//! TCP TAP transport (Gate 7 path): length-prefixed frames over a stream.
//!
//! Same frames as the loopback transport, real sockets instead of files —
//! this is the seam a real Fire TV client plugs into. One connection per
//! artefact transfer; per-frame acks only after validation.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use tapestry_core::protocol::frame::Frame;

const MAX_FRAME_SIZE: usize = 64 << 20; // 64 MiB, matches MAX_FRAME_PAYLOAD

/// Encode one frame with its 8-byte big-endian length prefix.
pub fn encode_frame(frame: &Frame) -> io::Result<Vec<u8>> {
    let json = frame
        .to_json()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if json.len() > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds 64 MiB limit",
        ));
    }
    let mut buf = Vec::with_capacity(8 + json.len());
    buf.extend_from_slice(&(json.len() as u64).to_be_bytes());
    buf.extend_from_slice(&json);
    Ok(buf)
}

/// Read one length-prefixed frame; caps size at 64 MiB.
pub fn read_frame<R: Read>(reader: &mut R) -> io::Result<Frame> {
    let mut len_bytes = [0u8; 8];
    reader.read_exact(&mut len_bytes)?;
    read_frame_payload(reader, len_bytes)
}

fn read_frame_or_eof<R: Read>(reader: &mut R) -> io::Result<Option<Frame>> {
    let mut len_bytes = [0u8; 8];
    match reader.read(&mut len_bytes[..1])? {
        0 => return Ok(None),
        1 => {}
        _ => unreachable!("read is limited to one byte"),
    }
    reader.read_exact(&mut len_bytes[1..])?;
    read_frame_payload(reader, len_bytes).map(Some)
}

fn read_frame_payload<R: Read>(reader: &mut R, len_bytes: [u8; 8]) -> io::Result<Frame> {
    let len = u64::from_be_bytes(len_bytes) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds 64 MiB limit",
        ));
    }
    let mut json = vec![0u8; len];
    reader.read_exact(&mut json)?;
    let frame =
        Frame::from_json(&json).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    frame
        .validate()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(frame)
}

/// Sender: write each frame, wait for a `+` ack before the next.
/// A `-` ack (validation failure) aborts the transfer.
pub fn send_frames(stream: &mut TcpStream, frames: &[Frame]) -> io::Result<()> {
    for frame in frames {
        let encoded = encode_frame(frame)?;
        stream.write_all(&encoded)?;
        stream.flush()?;
        let mut ack = [0u8; 1];
        stream.read_exact(&mut ack)?;
        if ack[0] != b'+' {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "receiver rejected frame seq {}: 0x{:02x}",
                    frame.seq, ack[0]
                ),
            ));
        }
    }
    Ok(())
}

/// Receiver side of one connection: validate seq contiguity from 1, ack
/// per frame, collect payloads. Returns the frame count.
pub fn serve_one(listener: &TcpListener, payloads: &mut Vec<Vec<u8>>) -> io::Result<usize> {
    let (mut stream, _) = listener.accept()?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;

    let mut expected_seq = 1u64;
    let mut count = 0usize;
    loop {
        let frame = match read_frame_or_eof(&mut stream) {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                let _ = stream.write_all(b"-");
                return Err(e);
            }
        };
        if frame.seq != expected_seq {
            let _ = stream.write_all(b"-");
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "non-contiguous frame seq {} (expected {expected_seq})",
                    frame.seq
                ),
            ));
        }
        stream.write_all(b"+")?;
        payloads.push(frame.payload);
        expected_seq += 1;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tapestry_core::ids::{DeviceId, SessionId};
    use tapestry_core::protocol::Channel;

    fn frame(seq: u64, payload: &[u8]) -> Frame {
        Frame::new(
            SessionId::generate(),
            Channel::Artefact,
            "chunk",
            seq,
            DeviceId::generate(),
            payload.to_vec(),
        )
    }

    type ServerHandle = std::thread::JoinHandle<io::Result<(usize, Vec<Vec<u8>>)>>;

    fn spawn_server() -> (std::net::SocketAddr, ServerHandle) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut buf: Vec<Vec<u8>> = Vec::new();
            let n = serve_one(&listener, &mut buf)?;
            Ok((n, buf))
        });
        (addr, handle)
    }

    #[test]
    fn roundtrip_multi_frame_over_tcp() {
        let (addr, handle) = spawn_server();

        let frames: Vec<Frame> = (1..=5).map(|i| frame(i, &[i as u8; 100])).collect();
        let stream = TcpStream::connect(addr).unwrap();
        // Sender must finish before serve_one returns on EOF.
        let sender = {
            let mut s = stream.try_clone().unwrap();
            std::thread::spawn(move || send_frames(&mut s, &frames))
        };
        // serve_one owns the accepted stream; the send side closes when done.
        sender.join().unwrap().unwrap();

        // Give the server thread a moment; it exits on read timeout/EOF of
        // its accepted stream once the sender drops its clone.
        std::thread::sleep(std::time::Duration::from_millis(200));
        let _ = stream.shutdown(std::net::Shutdown::Both);

        let (count, payloads) = handle.join().unwrap().unwrap();
        assert_eq!(count, 5);
        assert_eq!(payloads[0], vec![1u8; 100]);
        assert_eq!(payloads[4], vec![5u8; 100]);
    }

    #[test]
    fn non_contiguous_seq_is_rejected() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let mut buf = Vec::new();
            serve_one(&listener, &mut buf)
        });
        let stream = TcpStream::connect(addr).unwrap();
        let mut s = stream.try_clone().unwrap();
        let bad = vec![frame(1, b"a"), frame(3, b"b")];
        let err = send_frames(&mut s, &bad).unwrap_err();
        assert!(err.to_string().contains("rejected"));
        stream.shutdown(std::net::Shutdown::Both).unwrap();
        assert!(server.join().unwrap().is_err());
    }

    #[test]
    fn length_prefix_caps_insane_sizes() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(u64::MAX).to_be_bytes());
        let err = read_frame(&mut buf.as_slice()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn encode_read_roundtrip() {
        let f = frame(7, b"payload-bytes");
        let bytes = encode_frame(&f).unwrap();
        let back = read_frame(&mut bytes.as_slice()).unwrap();
        assert_eq!(f, back);
    }
}

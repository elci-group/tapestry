//! Ephemeral share tokens and QR payloads (Gate 8, directive §20).
//!
//! Short-lived, scoped, single-use; never vault credentials, never provider
//! API keys, never raw filesystem paths. QR generation is a deterministic
//! placeholder matrix — the real encoder is a later concern, the token
//! lifecycle is the security surface and it is complete here.

use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// One ephemeral share grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShareToken {
    pub token: String,
    pub artifact: String,
    pub expires_at_unix: u64,
    pub single_use: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ShareError {
    #[error("unknown share token")]
    UnknownToken,
    #[error("share token expired")]
    Expired,
    #[error("single-use token already redeemed")]
    AlreadyUsed,
    #[error("share payload contains forbidden material: {0}")]
    Forbidden(&'static str),
}

#[derive(Debug, Default)]
pub struct ShareRegistry {
    tokens: Vec<(ShareToken, bool)>, // (token, redeemed)
}

fn entropy_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
        .unwrap_or(0);
    nanos ^ (&nanos as *const _ as u64) ^ ((std::process::id() as u64) << 32)
}

impl ShareToken {
    /// Create a token from artifact id with `validity_secs` lifetime.
    pub fn new(artifact: &str, validity_secs: u64, single_use: bool) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(entropy_now().to_le_bytes());
        hasher.update(artifact.as_bytes());
        let digest = hex::encode(hasher.finalize());
        Self {
            token: digest[..32].to_owned(),
            artifact: artifact.to_owned(),
            expires_at_unix: now_unix() + validity_secs,
            single_use,
        }
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl ShareRegistry {
    pub fn issue(&mut self, artifact: &str, validity_secs: u64, single_use: bool) -> &ShareToken {
        self.tokens
            .push((ShareToken::new(artifact, validity_secs, single_use), false));
        &self.tokens.last().expect("just pushed").0
    }

    /// Redeem: returns the artifact id. Idempotency: a single-use token
    /// redeemed twice is `AlreadyUsed`; expiry is checked at redemption.
    pub fn redeem(&mut self, token: &str, now_unix: u64) -> Result<String, ShareError> {
        let entry = self
            .tokens
            .iter_mut()
            .find(|(t, _)| t.token == token)
            .ok_or(ShareError::UnknownToken)?;
        let share = &entry.0;
        let redeemed = entry.1;
        if now_unix >= share.expires_at_unix {
            return Err(ShareError::Expired);
        }
        if redeemed {
            return Err(ShareError::AlreadyUsed);
        }
        if share.single_use {
            entry.1 = true;
        }
        Ok(share.artifact.clone())
    }

    pub fn revoke(&mut self, token: &str) -> bool {
        let before = self.tokens.len();
        self.tokens.retain(|(t, _)| t.token != token);
        self.tokens.len() != before
    }
}

/// Build the share URL. Rejects forbidden payload material up front.
pub fn share_url(base: &str, token: &ShareToken) -> Result<String, ShareError> {
    let url = format!(
        "{}/s/{}?exp={}",
        base.trim_end_matches('/'),
        token.token,
        token.expires_at_unix
    );
    sanitize(&url)?;
    Ok(url)
}

/// Guard: no vault credentials, API keys, or permanent auth in share URLs.
/// Only http(s) share endpoints are acceptable carriers.
fn sanitize(url: &str) -> Result<(), ShareError> {
    let lower = url.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err(ShareError::Forbidden("non-http share carrier"));
    }
    for forbidden in [
        "api_key=",
        "apikey=",
        "vault=",
        "secret=",
        "password=",
        "path=/",
    ] {
        if lower.contains(forbidden) {
            return Err(ShareError::Forbidden(forbidden));
        }
    }
    Ok(())
}

/// Deterministic placeholder QR matrix (21×21, finder squares at three
/// corners, payload-hash-derived body). Stable across runs and machines —
/// the presentation layer renders it; the token above carries the security.
pub fn qr_matrix_placeholder(url: &str) -> Vec<Vec<bool>> {
    let size = 21;
    let mut matrix = vec![vec![false; size]; size];

    // Finder squares at (0,0), (0,14), (14,0): 7×7 rings.
    let place_finder = |m: &mut Vec<Vec<bool>>, r: usize, c: usize| {
        for i in 0..7usize {
            for j in 0..7usize {
                let edge = i == 0 || i == 6 || j == 0 || j == 6;
                let center = (2..=4).contains(&i) && (2..=4).contains(&j);
                m[r + i][c + j] = edge || center;
            }
        }
    };
    place_finder(&mut matrix, 0, 0);
    place_finder(&mut matrix, 0, 14);
    place_finder(&mut matrix, 14, 0);

    // Body: deterministic pattern from the URL hash.
    let digest = Sha256::digest(url.as_bytes());
    let mut bits = digest
        .iter()
        .flat_map(|byte| (0..8).map(move |bit| (byte >> bit) & 1 == 1));
    for (r, row) in matrix.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            let in_finder = (r < 8 && c < 8) || (r < 8 && c >= 13) || (r >= 13 && c < 8);
            if !in_finder {
                *cell = bits.next().unwrap_or(false);
            }
        }
    }
    matrix
}

/// ASCII rendering for terminal display (▓ / space).
pub fn qr_ascii(matrix: &[Vec<bool>]) -> String {
    let mut out = String::new();
    for row in matrix {
        for &cell in row {
            let _ = write!(out, "{}", if cell { "▓" } else { " " });
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_32_hex_and_deterministic_shape() {
        let before = now_unix();
        let t = ShareToken::new("art_abc", 300, true);
        assert_eq!(t.token.len(), 32);
        assert!(t.token.chars().all(|c| c.is_ascii_hexdigit()));
        // Lifetime is approximately right regardless of clock ticks.
        assert!((t.expires_at_unix - before) >= 299);
        assert!((t.expires_at_unix - before) <= 301);
    }

    #[test]
    fn redeem_single_use_twice_is_already_used() {
        let mut reg = ShareRegistry::default();
        let t = reg.issue("art_C", 600, true).clone();
        assert_eq!(reg.redeem(&t.token, now_unix()).unwrap(), "art_C");
        assert!(matches!(
            reg.redeem(&t.token, now_unix()),
            Err(ShareError::AlreadyUsed)
        ));
    }

    #[test]
    fn multi_use_token_can_redeem_repeatedly_until_expiry() {
        let mut reg = ShareRegistry::default();
        let t = reg.issue("art_A", 100, false).clone();
        let now = now_unix();
        reg.redeem(&t.token, now + 50).unwrap();
        reg.redeem(&t.token, now + 90).unwrap();
        assert!(matches!(
            reg.redeem(&t.token, now + 150),
            Err(ShareError::Expired)
        ));
    }

    #[test]
    fn unknown_and_revoked_tokens_rejected() {
        let mut reg = ShareRegistry::default();
        let t = reg.issue("art_A", 600, true).clone();
        assert!(matches!(
            reg.redeem("deadbeef".repeat(4).as_str(), 0),
            Err(ShareError::UnknownToken)
        ));
        assert!(reg.revoke(&t.token));
        assert!(matches!(
            reg.redeem(&t.token, 0),
            Err(ShareError::UnknownToken)
        ));
    }

    #[test]
    fn share_url_rejects_forbidden_payloads() {
        let t = ShareToken::new("art_A", 60, true);
        assert!(share_url("https://tap.example", &t).is_ok());
        assert!(share_url("https://tap.example?api_key=x", &t).is_err());
        assert!(share_url("https://tap.example?secret=x", &t).is_err());
        assert!(share_url("file:///etc/passwd", &t).is_err());
    }

    #[test]
    fn qr_matrix_deterministic_with_finders() {
        let a = qr_matrix_placeholder("https://tap.example/s/abc");
        let b = qr_matrix_placeholder("https://tap.example/s/abc");
        assert_eq!(a, b);
        assert_eq!(a.len(), 21);
        // Finder corners dark.
        assert!(a[0][0]);
        assert!(a[0][14]);
        assert!(a[14][0]);
        // Different payload, different body.
        let c = qr_matrix_placeholder("https://tap.example/s/xyz");
        assert_ne!(a, c);
        let ascii = qr_ascii(&a);
        assert!(ascii.contains('▓'));
    }
}

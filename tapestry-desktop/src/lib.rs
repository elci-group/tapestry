//! # tapestry-desktop
//!
//! The desktop coordination plane: the `tapestry` CLI, the loopback TAP
//! transport used until real devices arrive, scoped credentials, the
//! Playbook drive adapter, Mesut-shaped lanes, the knowledge journal,
//! resource visibility, share tokens and temperature policy.

pub mod auth;
pub mod cli;
pub mod knowledge;
pub mod lineage;
pub mod mesut;
pub mod playbook;
pub mod resources;
pub mod share;
pub mod slice;
pub mod tap_tcp;
pub mod temppolicy;
pub mod transport;

/// Process-wide data layout for the desktop coordinator.
#[derive(Debug, Clone)]
pub struct DataDirs {
    pub root: std::path::PathBuf,
}

impl DataDirs {
    /// `TAPESTRY_DATA` if set, else `<cwd>/tapestry-data`.
    pub fn resolve() -> Self {
        let root = std::env::var_os("TAPESTRY_DATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("tapestry-data"));
        Self { root }
    }

    pub fn vault(&self) -> std::path::PathBuf {
        self.root.join("vault")
    }

    pub fn knowledge(&self) -> std::path::PathBuf {
        self.root.join("knowledge")
    }

    pub fn drives(&self) -> std::path::PathBuf {
        self.root.join("drives")
    }

    pub fn transport(&self) -> std::path::PathBuf {
        self.root.join("transport")
    }

    pub fn artefacts(&self) -> std::path::PathBuf {
        self.root.join("artefacts")
    }
}

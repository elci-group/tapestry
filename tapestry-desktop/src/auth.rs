//! Scoped device credentials (Gate 2, directive §11).
//!
//! DreamSequence-shaped: the traits are the contract; the POC provider
//! reads token material from files. TV clients receive only scoped
//! credentials — never vault or provider keys.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// Actions a device may or may not perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Denied-by-default set (§11): never granted to a television.
    ReceiveSession,
    ReceiveArtefact,
    ReadTelemetry,
    SubmitReview,
    RequestRegeneration,
    ReadVault,
    ProviderApi,
    ShellExecute,
    RepositoryRead,
    DeploymentAuthorize,
}

impl Scope {
    /// Scopes a presentation device legitimately holds.
    pub fn device_scopes() -> &'static [Scope] {
        &[
            Scope::ReceiveSession,
            Scope::ReceiveArtefact,
            Scope::ReadTelemetry,
            Scope::SubmitReview,
            Scope::RequestRegeneration,
        ]
    }

    pub fn is_denied_by_default(self) -> bool {
        matches!(
            self,
            Scope::ReadVault
                | Scope::ProviderApi
                | Scope::ShellExecute
                | Scope::RepositoryRead
                | Scope::DeploymentAuthorize
        )
    }

    fn token_file_name(self) -> &'static str {
        match self {
            Scope::ReceiveSession => "receive_session",
            Scope::ReceiveArtefact => "receive_artefact",
            Scope::ReadTelemetry => "read_telemetry",
            Scope::SubmitReview => "submit_review",
            Scope::RequestRegeneration => "request_regeneration",
            Scope::ReadVault => "read_vault",
            Scope::ProviderApi => "provider_api",
            Scope::ShellExecute => "shell_execute",
            Scope::RepositoryRead => "repository_read",
            Scope::DeploymentAuthorize => "deployment_authorize",
        }
    }
}

/// A device credential with explicit scopes and expiry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedCredential {
    pub device_id: tapestry_core::ids::DeviceId,
    pub scopes: Vec<Scope>,
    pub issued_at_unix: u64,
    pub expires_at_unix: u64,
    /// Digest of the token material — proofs of possession, never the token.
    pub token_digest: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("scope {0:?} is denied by default and cannot be granted to a device")]
    DeniedByDefault(Scope),
    #[error("credential for scope {0:?} not found in vault")]
    NotFound(Scope),
    #[error("credential expired at {0}")]
    Expired(u64),
    #[error("vault read failed: {0}")]
    VaultRead(String),
}

/// Who a device is.
pub trait IdentityProvider {
    fn identity(&self) -> tapestry_core::identity::DeviceIdentity;
}

/// Where scoped credentials come from.
pub trait CredentialProvider {
    fn credential(&self, scope: Scope, now_unix: u64) -> Result<String, AuthError>;
}

/// Who may do what.
pub trait SessionAuthorizer {
    fn authorize(&self, credential: &ScopedCredential, scope: Scope, now_unix: u64) -> bool;
}

/// POC provider: token material lives as files under a data dir
/// (`<data_dir>/<scope>.token`), standing in for DreamSequence vaults.
/// No network; no provider keys ever cross this interface.
#[derive(Debug, Clone)]
pub struct DreamSequenceVaultProvider {
    pub data_dir: PathBuf,
}

impl DreamSequenceVaultProvider {
    pub fn issue_scoped_credential(
        &self,
        device_id: tapestry_core::ids::DeviceId,
        now_unix: u64,
        validity_secs: u64,
    ) -> Result<ScopedCredential, AuthError> {
        let mut token_digest = Sha256::new();
        let mut scopes = Vec::new();
        for scope in Scope::device_scopes() {
            let token = self.credential(*scope, now_unix)?;
            token_digest.update(token.as_bytes());
            token_digest.update(b"\n");
            scopes.push(*scope);
        }
        Ok(ScopedCredential {
            device_id,
            scopes,
            issued_at_unix: now_unix,
            expires_at_unix: now_unix + validity_secs,
            token_digest: hex::encode(token_digest.finalize()),
        })
    }
}

impl CredentialProvider for DreamSequenceVaultProvider {
    fn credential(&self, scope: Scope, _now_unix: u64) -> Result<String, AuthError> {
        if scope.is_denied_by_default() {
            // The vault refuses to hand these to any device, ever.
            return Err(AuthError::DeniedByDefault(scope));
        }
        let path = self
            .data_dir
            .join(format!("{}.token", scope.token_file_name()));
        std::fs::read_to_string(&path)
            .map(|s| s.trim().to_owned())
            .map_err(|_| AuthError::NotFound(scope))
    }
}

/// Authorizer: scope present, not expired, not denied-by-default.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScopedAuthorizer;

impl SessionAuthorizer for ScopedAuthorizer {
    fn authorize(&self, credential: &ScopedCredential, scope: Scope, now_unix: u64) -> bool {
        if scope.is_denied_by_default() {
            return false;
        }
        credential.scopes.contains(&scope) && now_unix < credential.expires_at_unix
    }
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Write the POC vault token files for a device under `data_dir`.
/// Tokens are random-ish but deterministic per (device, scope) — enough for
/// a POC; real entropy belongs to DreamSequence.
pub fn seed_vault(
    data_dir: &Path,
    device_id: &tapestry_core::ids::DeviceId,
) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    for scope in Scope::device_scopes() {
        let material = format!("poc-token:{}:{}", device_id, scope.token_file_name());
        let digest = hex::encode(Sha256::digest(material.as_bytes()));
        std::fs::write(
            data_dir.join(format!("{}.token", scope.token_file_name())),
            digest,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> (tempfile::TempDir, tapestry_core::ids::DeviceId) {
        let dir = tempfile::tempdir().unwrap();
        let device = tapestry_core::ids::DeviceId::generate();
        seed_vault(dir.path(), &device).unwrap();
        (dir, device)
    }

    #[test]
    fn device_scopes_exclude_denied_defaults() {
        for scope in Scope::device_scopes() {
            assert!(!scope.is_denied_by_default());
        }
        assert!(Scope::ProviderApi.is_denied_by_default());
        assert!(Scope::ShellExecute.is_denied_by_default());
    }

    #[test]
    fn vault_issues_credential_with_device_scopes() {
        let (dir, device) = vault();
        let provider = DreamSequenceVaultProvider {
            data_dir: dir.path().into(),
        };
        let cred = provider
            .issue_scoped_credential(device, 1_000, 3_600)
            .unwrap();
        assert_eq!(cred.scopes.len(), Scope::device_scopes().len());
        assert_eq!(cred.expires_at_unix, 4_600);
        assert_eq!(cred.token_digest.len(), 64);
    }

    #[test]
    fn authorizer_grants_then_expires() {
        let (dir, device) = vault();
        let provider = DreamSequenceVaultProvider {
            data_dir: dir.path().into(),
        };
        let cred = provider
            .issue_scoped_credential(device, 1_000, 100)
            .unwrap();
        let auth = ScopedAuthorizer;
        assert!(auth.authorize(&cred, Scope::SubmitReview, 1_050));
        assert!(!auth.authorize(&cred, Scope::SubmitReview, 1_200)); // expired
        assert!(!auth.authorize(&cred, Scope::ReadVault, 1_050)); // denied by default
        assert!(!auth.authorize(&cred, Scope::ProviderApi, 1_050));
    }

    #[test]
    fn vault_refuses_denied_scopes_even_if_file_existed() {
        let dir = tempfile::tempdir().unwrap();
        let provider = DreamSequenceVaultProvider {
            data_dir: dir.path().into(),
        };
        let err = provider.credential(Scope::ReadVault, 0).unwrap_err();
        assert!(matches!(err, AuthError::DeniedByDefault(Scope::ReadVault)));
    }

    #[test]
    fn missing_token_file_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let provider = DreamSequenceVaultProvider {
            data_dir: dir.path().into(),
        };
        let err = provider.credential(Scope::SubmitReview, 0).unwrap_err();
        assert!(matches!(err, AuthError::NotFound(_)));
    }

    #[test]
    fn credential_serde_roundtrip() {
        let (dir, device) = vault();
        let provider = DreamSequenceVaultProvider {
            data_dir: dir.path().into(),
        };
        let cred = provider.issue_scoped_credential(device, 0, 60).unwrap();
        let back: ScopedCredential =
            serde_json::from_str(&serde_json::to_string(&cred).unwrap()).unwrap();
        assert_eq!(cred, back);
    }
}

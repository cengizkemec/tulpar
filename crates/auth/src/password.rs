//! Argon2id password hashing.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};
use tokio::task;

use crate::AuthError;

/// Password hashing and verification service.
#[derive(Clone)]
pub struct PasswordService {
    argon2: Argon2<'static>,
    dummy_hash: String,
}

impl PasswordService {
    pub fn new() -> Result<Self, AuthError> {
        let params = Params::new(19 * 1024, 2, 1, Some(32))
            .map_err(|error| AuthError::PasswordHash(error.to_string()))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let dummy_hash = hash_with(&argon2, "tulpar-dummy-password")?;
        Ok(Self { argon2, dummy_hash })
    }

    pub async fn hash(&self, password: String) -> Result<String, AuthError> {
        let argon2 = self.argon2.clone();
        task::spawn_blocking(move || hash_with(&argon2, &password))
            .await
            .map_err(|error| AuthError::Internal(format!("password task failed: {error}")))?
    }

    /// Verify a password. Unknown users verify against a dummy hash to reduce timing leaks.
    pub async fn verify(&self, password: String, stored_hash: Option<String>) -> bool {
        let hash = stored_hash.unwrap_or_else(|| self.dummy_hash.clone());
        let argon2 = self.argon2.clone();
        task::spawn_blocking(move || {
            let parsed = match PasswordHash::new(&hash) {
                Ok(parsed) => parsed,
                Err(_) => return false,
            };
            argon2.verify_password(password.as_bytes(), &parsed).is_ok()
        })
        .await
        .unwrap_or(false)
    }
}

fn hash_with(argon2: &Argon2<'_>, password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| AuthError::PasswordHash(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hashes_and_verifies() {
        let service = PasswordService::new().expect("password service");
        let hash = service
            .hash("correct horse battery staple".into())
            .await
            .unwrap();
        assert!(
            service
                .verify("correct horse battery staple".into(), Some(hash))
                .await
        );
    }

    #[tokio::test]
    async fn rejects_wrong_password() {
        let service = PasswordService::new().expect("password service");
        let hash = service
            .hash("correct horse battery staple".into())
            .await
            .unwrap();
        assert!(!service.verify("wrong password".into(), Some(hash)).await);
    }

    #[tokio::test]
    async fn unknown_user_uses_dummy_hash() {
        let service = PasswordService::new().expect("password service");
        assert!(!service.verify("anything".into(), None).await);
    }
}

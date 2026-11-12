//! Shared validation, normalization, and token helpers.

#![forbid(unsafe_code)]

use base64::Engine;
use email_address::EmailAddress;
use rand::RngCore;
use sha2::{Digest, Sha256};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

/// Errors returned by username validation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum UsernameError {
    #[error("username must be between 2 and 32 characters")]
    Length,
    #[error(
        "username may contain only lowercase letters, numbers, dots, underscores, and hyphens"
    )]
    Characters,
    #[error("username cannot start or end with a dot or hyphen")]
    EdgeCharacter,
    #[error("username is reserved")]
    Reserved,
}

/// Normalize a username before validation and storage.
pub fn normalize_username(value: &str) -> String {
    value.trim().nfc().collect::<String>().to_lowercase()
}

/// Validate a normalized username.
pub fn validate_username(value: &str) -> Result<(), UsernameError> {
    let length = value.chars().count();
    if !(2..=32).contains(&length) {
        return Err(UsernameError::Length);
    }

    if !value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
    {
        return Err(UsernameError::Characters);
    }

    if value.starts_with('.')
        || value.starts_with('-')
        || value.ends_with('.')
        || value.ends_with('-')
    {
        return Err(UsernameError::EdgeCharacter);
    }

    const RESERVED: &[&str] = &[
        "admin",
        "everyone",
        "here",
        "me",
        "moderator",
        "owner",
        "support",
        "system",
        "tulpar",
    ];
    if RESERVED.contains(&value) {
        return Err(UsernameError::Reserved);
    }

    Ok(())
}

/// Normalize an email address to trimmed lowercase NFC text.
pub fn normalize_email(value: &str) -> String {
    value.trim().nfc().collect::<String>().to_lowercase()
}

/// Validate an email address using a dedicated parser.
pub fn validate_email(value: &str) -> bool {
    if value.len() > 254 || value.chars().any(char::is_whitespace) {
        return false;
    }
    EmailAddress::is_valid(value)
}

/// Generate a URL-safe random token with an explicit byte length.
pub fn random_token(bytes: usize) -> String {
    let mut buffer = vec![0_u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buffer);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buffer)
}

/// Hash a token before persistence.
pub fn token_hash(token: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher.finalize().into()
}

/// Normalize arbitrary user text to NFC and trim surrounding whitespace.
pub fn normalize_text(value: &str) -> String {
    value.trim().nfc().collect()
}

/// Normalize a password to NFC without trimming it.
pub fn normalize_password(value: &str) -> String {
    value.nfc().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_normalization_and_validation() {
        assert_eq!(normalize_username("  Ada.Lovelace  "), "ada.lovelace");
        assert!(validate_username("ada.lovelace").is_ok());
        assert!(validate_username("a").is_err());
        assert!(validate_username("Ada").is_err());
        assert!(validate_username("-ada").is_err());
        assert!(validate_username("everyone").is_err());
        assert!(validate_username("ada lovelace").is_err());
    }

    #[test]
    fn email_validation() {
        assert_eq!(normalize_email(" Ada@Example.COM "), "ada@example.com");
        assert!(validate_email("ada@example.com"));
        assert!(!validate_email("not-an-email"));
        assert!(!validate_email("ada @example.com"));
    }

    #[test]
    fn token_hashes_are_stable_and_random_tokens_are_unique() {
        let a = random_token(32);
        let b = random_token(32);
        assert_ne!(a, b);
        assert_eq!(token_hash(&a), token_hash(&a));
        assert_ne!(token_hash(&a), token_hash(&b));
        assert_eq!(a.len(), 43);
    }

    #[test]
    fn password_normalization_preserves_surrounding_space() {
        assert_eq!(normalize_password(" pass "), " pass ");
    }
}

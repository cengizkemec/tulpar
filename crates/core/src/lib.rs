//! Core identifiers and protocol constants shared by Tulpar crates.

#![forbid(unsafe_code)]

use uuid::Uuid;

/// Public REST API version.
pub const API_VERSION: &str = "v1";

/// WebSocket gateway protocol version.
pub const GATEWAY_PROTOCOL_VERSION: u16 = 1;

/// Maximum length of a user-visible display name.
pub const DISPLAY_NAME_MAX_LENGTH: usize = 64;

/// Maximum length of a username.
pub const USERNAME_MAX_LENGTH: usize = 32;

/// Minimum length of a username.
pub const USERNAME_MIN_LENGTH: usize = 2;

/// Generate a time-ordered public UUID.
pub fn new_id() -> Uuid {
    Uuid::now_v7()
}

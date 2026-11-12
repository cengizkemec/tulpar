//! Local object storage and upload validation.

#![forbid(unsafe_code)]

use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("storage I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid upload: {0}")]
    Invalid(String),
    #[error("object not found")]
    NotFound,
}

#[derive(Debug, Clone)]
pub struct StoredObject {
    pub object_key: String,
    pub size_bytes: u64,
    pub sha256: Vec<u8>,
}

#[derive(Clone)]
pub struct StorageService {
    root: PathBuf,
    max_upload_bytes: u64,
}

impl StorageService {
    pub fn new(root: impl Into<PathBuf>, max_upload_bytes: u64) -> Self {
        Self {
            root: root.into(),
            max_upload_bytes,
        }
    }

    pub fn max_upload_bytes(&self) -> u64 {
        self.max_upload_bytes
    }

    pub fn validate_upload(
        &self,
        filename: &str,
        content_type: &str,
        size_bytes: u64,
    ) -> Result<(), StorageError> {
        if size_bytes == 0 {
            return Err(StorageError::Invalid("file is empty".into()));
        }
        if size_bytes > self.max_upload_bytes {
            return Err(StorageError::Invalid(format!(
                "file exceeds the {} byte upload limit",
                self.max_upload_bytes
            )));
        }
        if sanitize_filename(filename).is_empty() {
            return Err(StorageError::Invalid("file name is invalid".into()));
        }
        if !allowed_content_type(content_type) {
            return Err(StorageError::Invalid(format!(
                "unsupported content type: {content_type}"
            )));
        }
        Ok(())
    }

    pub async fn store(
        &self,
        owner_id: Uuid,
        file_id: Uuid,
        bytes: &[u8],
    ) -> Result<StoredObject, StorageError> {
        let directory = self.root.join(owner_id.to_string());
        tokio::fs::create_dir_all(&directory).await?;
        let object_key = format!("{owner_id}/{file_id}");
        let path = directory.join(file_id.to_string());
        tokio::fs::write(&path, bytes).await?;
        let hash = Sha256::digest(bytes).to_vec();
        Ok(StoredObject {
            object_key,
            size_bytes: bytes.len() as u64,
            sha256: hash,
        })
    }

    pub async fn read(&self, object_key: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.resolve(object_key)?;
        match tokio::fs::read(path).await {
            Ok(bytes) => Ok(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(StorageError::NotFound)
            }
            Err(error) => Err(StorageError::Io(error)),
        }
    }

    pub async fn delete(&self, object_key: &str) -> Result<(), StorageError> {
        let path = self.resolve(object_key)?;
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(StorageError::Io(error)),
        }
    }

    fn resolve(&self, object_key: &str) -> Result<PathBuf, StorageError> {
        let relative = Path::new(object_key);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(StorageError::Invalid("invalid object key".into()));
        }
        Ok(self.root.join(relative))
    }
}

pub fn sanitize_filename(filename: &str) -> String {
    let name = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename)
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    name.trim().chars().take(255).collect()
}

pub fn allowed_content_type(content_type: &str) -> bool {
    matches!(
        content_type,
        "image/png"
            | "image/jpeg"
            | "image/gif"
            | "image/webp"
            | "application/pdf"
            | "text/plain"
            | "application/zip"
            | "audio/mpeg"
            | "audio/ogg"
            | "video/mp4"
            | "video/webm"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filenames_cannot_escape_the_storage_root() {
        assert_eq!(sanitize_filename("../../secret.txt"), "secret.txt");
        assert_eq!(sanitize_filename("C:\\temp\\file.txt"), "file.txt");
        assert!(sanitize_filename("   ").is_empty());
    }

    #[test]
    fn content_type_allowlist_is_explicit() {
        assert!(allowed_content_type("image/png"));
        assert!(allowed_content_type("application/pdf"));
        assert!(!allowed_content_type("text/html"));
        assert!(!allowed_content_type("application/octet-stream"));
    }

    #[test]
    fn upload_validation_rejects_empty_and_large_files() {
        let service = StorageService {
            root: PathBuf::from("unused"),
            max_upload_bytes: 10,
        };
        assert!(service.validate_upload("a.txt", "text/plain", 1).is_ok());
        assert!(service.validate_upload("a.txt", "text/plain", 0).is_err());
        assert!(service.validate_upload("a.txt", "text/plain", 11).is_err());
        assert!(service.validate_upload("a.txt", "text/html", 1).is_err());
    }
}

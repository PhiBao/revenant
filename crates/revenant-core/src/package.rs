//! Deterministic ZIP packaging for a resurrected site.
//!
//! The same entries always produce the same bytes: entries are sorted by path, timestamps are
//! fixed to the ZIP epoch, and files are stored without compression. Paths are sanitized before
//! they reach the archive (see `.kiro/steering/security.md`).

use std::io::{Cursor, Write};

use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// One file to place in the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Relative path inside the archive. Must pass `sanitize_path`.
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    Empty,
    Absolute,
    Backslash,
    DriveOrColon,
    Traversal,
    EmptySegment,
    ControlCharacter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageError {
    InvalidPath { path: String, reason: PathError },
    DuplicatePath(String),
    Zip(String),
}

/// Validate and normalize a relative archive path.
///
/// Accepts `a/b/c.gif`. Rejects absolute paths, backslashes, drive prefixes, `..` segments,
/// empty segments, `.` segments, and control characters.
pub fn sanitize_path(raw: &str) -> Result<String, PathError> {
    if raw.is_empty() {
        return Err(PathError::Empty);
    }
    if raw.starts_with('/') {
        return Err(PathError::Absolute);
    }
    if raw.contains('\\') {
        return Err(PathError::Backslash);
    }
    if raw.contains(':') {
        return Err(PathError::DriveOrColon);
    }
    if raw.chars().any(|c| c.is_control()) {
        return Err(PathError::ControlCharacter);
    }
    let mut segments = Vec::new();
    for seg in raw.split('/') {
        match seg {
            "" => return Err(PathError::EmptySegment),
            "." | ".." => return Err(PathError::Traversal),
            s => segments.push(s),
        }
    }
    Ok(segments.join("/"))
}

/// Build a deterministic ZIP archive from entries.
pub fn build_zip(entries: &[Entry]) -> Result<Vec<u8>, PackageError> {
    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));

    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let mut last: Option<&str> = None;
    for entry in sorted {
        let path = sanitize_path(&entry.path).map_err(|reason| PackageError::InvalidPath {
            path: entry.path.clone(),
            reason,
        })?;
        if last == Some(path.as_str()) {
            return Err(PackageError::DuplicatePath(path));
        }
        last = Some(entry.path.as_str());
        writer
            .start_file(path.as_str(), options)
            .map_err(|e| PackageError::Zip(e.to_string()))?;
        writer
            .write_all(&entry.bytes)
            .map_err(|e| PackageError::Zip(e.to_string()))?;
    }
    let cursor = writer
        .finish()
        .map_err(|e| PackageError::Zip(e.to_string()))?;
    Ok(cursor.into_inner())
}

/// Lowercase hex SHA-256 of a byte slice.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_relative_paths() {
        assert_eq!(sanitize_path("images/1.gif").as_deref(), Ok("images/1.gif"));
        assert_eq!(
            sanitize_path("briefings.html").as_deref(),
            Ok("briefings.html")
        );
    }

    #[test]
    fn rejects_unsafe_paths() {
        assert_eq!(sanitize_path(""), Err(PathError::Empty));
        assert_eq!(sanitize_path("/etc/passwd"), Err(PathError::Absolute));
        assert_eq!(sanitize_path("..\\x"), Err(PathError::Backslash));
        assert_eq!(sanitize_path("C:x"), Err(PathError::DriveOrColon));
        assert_eq!(sanitize_path("a/../b"), Err(PathError::Traversal));
        assert_eq!(sanitize_path("a//b"), Err(PathError::EmptySegment));
        assert_eq!(sanitize_path("a/\u{0}b"), Err(PathError::ControlCharacter));
    }

    #[test]
    fn duplicate_paths_are_rejected() {
        let e = vec![
            Entry {
                path: "a".into(),
                bytes: vec![1],
            },
            Entry {
                path: "a".into(),
                bytes: vec![2],
            },
        ];
        assert_eq!(build_zip(&e), Err(PackageError::DuplicatePath("a".into())));
    }

    #[test]
    fn sha256_matches_known_value() {
        // SHA-256 of the empty input.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}

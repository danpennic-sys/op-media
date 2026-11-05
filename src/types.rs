//! Core types for the op-media pipeline.
//!
//! The only type the host application is required to supply is [`Marker`].
//! Everything else is produced or consumed by this crate or by platform
//! adapters (`Store`, `Signer`, `Scrubber`, `Inference`).

use serde::{Deserialize, Serialize};
use std::fmt;

/// Opaque session identifier supplied by the host.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Kind of media being ingested.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum MediaKind {
    Photo = 0,
    Video = 1,
}

/// Quality of the location fix attached to a marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum FixQuality {
    Green = 0,
    Amber = 1,
    Red = 2,
    Unknown = 3,
}

/// Opaque handle returned by [`crate::Store::put`]. Never contains pixel data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handle(pub String);

impl Handle {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

/// The **only** data the core application must provide for an ingest.
///
/// Position is optional and is dropped unless `Config::record_position` is set.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Marker {
    pub id: u64,
    pub t_ns: u64,
    pub fix: FixQuality,
    pub pos: Option<(f64, f64)>,
}

/// Raw media bytes ready for scrubbing / storage.
#[derive(Clone, Debug)]
pub struct MediaItem {
    pub session: SessionId,
    pub kind: MediaKind,
    pub captured_ns: u64,
    pub bytes: Vec<u8>,
}

/// RGBA frame for on-device inference (pose / detection).
#[derive(Clone, Debug)]
pub struct Frame {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// Single keypoint produced by an [`crate::Inference`] implementation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Keypoint {
    pub id: u8,
    pub x: f32,
    pub y: f32,
    pub conf: f32,
}

/// Runtime configuration. Defaults are privacy-maximal and fail-closed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Master switch. Default: `false`.
    pub enabled: bool,
    /// Strip EXIF / GPS / device tags. Default: `true`.
    pub strip_metadata: bool,
    /// Permit unscrubbed media when `strip_metadata` is false. Default: `false`.
    pub allow_unscrubbed: bool,
    /// Keep original bytes in addition to scrubbed derivatives. Default: `false`.
    pub keep_originals: bool,
    /// Record `Marker::pos` into the chain. Default: `false`.
    pub record_position: bool,
    /// Attempt network upload (not implemented in this crate). Default: `false`.
    pub upload: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            strip_metadata: true,
            allow_unscrubbed: false,
            keep_originals: false,
            record_position: false,
            upload: false,
        }
    }
}

/// Signed, hash-chained manifest entry. **Never contains pixels.**
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub session: SessionId,
    pub marker_id: u64,
    pub kind: MediaKind,
    pub captured_ns: u64,
    pub fix: FixQuality,
    pub pos: Option<(f64, f64)>,
    pub content_sha256: [u8; 32],
    pub handle: Handle,
    pub prev_hash: [u8; 32],
    pub hash: [u8; 32],
    pub sig: Vec<u8>,
}

/// Errors from the storage adapter.
#[derive(Debug, thiserror::Error)]
#[error("store error: {0}")]
pub struct StoreError(pub String);

impl StoreError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

/// Errors from the scrubber.
#[derive(Debug, thiserror::Error)]
#[error("scrub error: {0}")]
pub struct ScrubError(pub String);

impl ScrubError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

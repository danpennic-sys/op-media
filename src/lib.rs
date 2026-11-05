//! # op-media
//!
//! Privacy-first, fail-closed media ingestion for operator applications.
//!
//! The host application supplies only a [`Marker`] and a session identifier.
//! Platform shells implement the four traits ([`Store`], [`Signer`],
//! [`Scrubber`], [`Inference`]) via FFI; this crate never touches the OS
//! directly.
//!
//! ## Pipeline
//!
//! ```text
//! scrub → SHA-256 → encrypted put → hash-chain (prev_hash) → sign → append
//! ```
//!
//! Defaults are privacy-maximal: the module is **disabled**, metadata is
//! stripped, position is dropped, originals are discarded, and upload is off.
//!
//! ## Quick start (in-memory adapters for tests)
//!
//! ```rust
//! use op_media::*;
//!
//! // See the integration tests for a complete working example with
//! // MemStore + a trivial Signer + JpegScrubber.
//! ```
//!
//! ## Feature flags
//!
//! - `pose` – reserved for future Inference-backed FrameRecord output.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod chain;
mod ingest;
mod scrub;
mod types;

pub use chain::{verify_chain, ChainError};
pub use ingest::{IngestError, Ingestor};
pub use scrub::JpegScrubber;
pub use types::*;

// ---------------------------------------------------------------------------
// Platform adapter traits
// ---------------------------------------------------------------------------

/// Encrypted, local-only storage backend.
///
/// Implementations live in the iOS / Android shell and are reached via FFI.
pub trait Store {
    /// Write scrubbed media bytes. Returns an opaque handle that will be
    /// recorded in the chain. The handle must never contain pixel data.
    fn put(&mut self, kind: MediaKind, bytes: &[u8]) -> Result<Handle, StoreError>;

    /// Append a fully signed record to the durable log for its session.
    fn append_record(&mut self, rec: &Record) -> Result<(), StoreError>;

    /// Return the hash of the most recent record for the given session,
    /// or `None` if the session has no records yet.
    fn last_record_hash(&self, session: &SessionId) -> Option<[u8; 32]>;
}

/// Produces a signature over a 32-byte message (normally a record hash).
pub trait Signer {
    /// Sign the given message. The returned bytes are stored in `Record::sig`.
    fn sign(&self, msg: &[u8; 32]) -> Vec<u8>;
}

/// Removes EXIF / GPS / device tags and returns clean bytes.
///
/// Must fail closed (`Err`) on unknown or unsupported formats so that
/// metadata can never leak.
pub trait Scrubber {
    /// Scrub the supplied bytes according to the media kind.
    fn scrub(&self, kind: MediaKind, bytes: &[u8]) -> Result<Vec<u8>, ScrubError>;
}

/// On-device vision model (pose, detection, …).
///
/// Backed by MediaPipe, TFLite, Core ML, etc. in the platform shell.
pub trait Inference {
    /// Run inference on a single frame and return detected keypoints.
    fn run(&mut self, t_ns: u64, frame: &Frame) -> Vec<Keypoint>;
}

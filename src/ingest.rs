//! High-level ingestion pipeline.
//!
//! Flow: policy check → scrub → content hash → store → chain → sign → append.
//! Every step fails closed.

use crate::chain::record_hash;
use crate::types::*;
use crate::{Scrubber, Signer, Store};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Errors that can occur during ingest.
#[derive(Debug, Error)]
pub enum IngestError {
    #[error("module is disabled")]
    Disabled,
    #[error("policy violation: {0}")]
    Policy(&'static str),
    #[error(transparent)]
    Scrub(#[from] ScrubError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Generic ingestor parameterised by the three platform adapters.
pub struct Ingestor<S: Store, G: Signer, X: Scrubber> {
    cfg: Config,
    store: S,
    signer: G,
    scrubber: X,
}

impl<S: Store, G: Signer, X: Scrubber> Ingestor<S, G, X> {
    /// Construct an ingestor after validating the supplied configuration.
    pub fn new(cfg: Config, store: S, signer: G, scrubber: X) -> Result<Self, IngestError> {
        if !cfg.strip_metadata && !cfg.allow_unscrubbed {
            return Err(IngestError::Policy(
                "unscrubbed media requires allow_unscrubbed = true",
            ));
        }
        if cfg.upload {
            return Err(IngestError::Policy(
                "upload path is not implemented in this crate",
            ));
        }
        Ok(Self {
            cfg,
            store,
            signer,
            scrubber,
        })
    }

    /// Borrow the underlying store (useful for tests and inspection).
    pub fn store(&self) -> &S {
        &self.store
    }

    /// Mutable access to the store.
    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    /// Current configuration.
    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// Full pipeline: scrub → hash → store → chain → sign → persist.
    pub fn ingest(&mut self, item: MediaItem, marker: &Marker) -> Result<Record, IngestError> {
        if !self.cfg.enabled {
            return Err(IngestError::Disabled);
        }

        let clean = if self.cfg.strip_metadata {
            self.scrubber.scrub(item.kind, &item.bytes)?
        } else {
            item.bytes
        };

        let content_sha256: [u8; 32] = Sha256::digest(&clean).into();
        let handle = self.store.put(item.kind, &clean)?;

        let prev_hash = self
            .store
            .last_record_hash(&item.session)
            .unwrap_or([0u8; 32]);

        let mut rec = Record {
            session: item.session,
            marker_id: marker.id,
            kind: item.kind,
            captured_ns: item.captured_ns,
            fix: marker.fix,
            pos: if self.cfg.record_position {
                marker.pos
            } else {
                None
            },
            content_sha256,
            handle,
            prev_hash,
            hash: [0u8; 32],
            sig: Vec::new(),
        };

        rec.hash = record_hash(&rec);
        rec.sig = self.signer.sign(&rec.hash);
        self.store.append_record(&rec)?;

        Ok(rec)
    }
}

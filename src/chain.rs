//! Hash-chain construction and verification for [`Record`]s.
//!
//! Each record commits to the previous record's hash, producing a
//! tamper-evident log scoped to a single session.

use crate::types::*;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Canonical, length-prefixed, domain-separated encoding.
///
/// The encoding is deliberately unambiguous so that records cannot be
/// reinterpreted under a different schema.
pub(crate) fn record_hash(r: &Record) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"op-media/record/v1");
    h.update((r.session.0.len() as u32).to_be_bytes());
    h.update(r.session.0.as_bytes());
    h.update(r.marker_id.to_be_bytes());
    h.update([r.kind as u8, r.fix as u8]);
    h.update(r.captured_ns.to_be_bytes());
    match r.pos {
        Some((lat, lon)) => {
            h.update([1u8]);
            h.update(lat.to_be_bytes());
            h.update(lon.to_be_bytes());
        }
        None => h.update([0u8]),
    }
    h.update(r.content_sha256);
    h.update((r.handle.0.len() as u32).to_be_bytes());
    h.update(r.handle.0.as_bytes());
    h.update(r.prev_hash);
    h.finalize().into()
}

/// Verification failures for a chain of records.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ChainError {
    #[error("broken link at index {0}")]
    BrokenLink(usize),
    #[error("bad content hash at index {0}")]
    BadHash(usize),
    #[error("bad signature at index {0}")]
    BadSignature(usize),
}

/// Verify linkage, content hashes, and signatures for an ordered slice of records.
///
/// `verify_sig` is supplied by the caller so that the public-key material
/// never has to live inside this crate.
pub fn verify_chain(
    recs: &[Record],
    verify_sig: impl Fn(&[u8; 32], &[u8]) -> bool,
) -> Result<(), ChainError> {
    let mut prev = [0u8; 32];
    for (i, r) in recs.iter().enumerate() {
        if r.prev_hash != prev {
            return Err(ChainError::BrokenLink(i));
        }
        if record_hash(r) != r.hash {
            return Err(ChainError::BadHash(i));
        }
        if !verify_sig(&r.hash, &r.sig) {
            return Err(ChainError::BadSignature(i));
        }
        prev = r.hash;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_record(prev: [u8; 32], marker_id: u64) -> Record {
        let mut r = Record {
            session: SessionId::new("test"),
            marker_id,
            kind: MediaKind::Photo,
            captured_ns: 0,
            fix: FixQuality::Green,
            pos: None,
            content_sha256: [0; 32],
            handle: Handle::new("h0"),
            prev_hash: prev,
            hash: [0; 32],
            sig: Vec::new(),
        };
        r.hash = record_hash(&r);
        r
    }

    #[test]
    fn empty_chain_ok() {
        assert!(verify_chain(&[], |_, _| true).is_ok());
    }

    #[test]
    fn single_record_links_to_zero() {
        let r = dummy_record([0; 32], 1);
        assert!(verify_chain(&[r], |_, _| true).is_ok());
    }
}

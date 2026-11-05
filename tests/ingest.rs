//! End-to-end tests of the ingest pipeline using pure in-memory adapters.

use op_media::*;

// ---------------------------------------------------------------------------
// Test doubles
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MemStore {
    blobs: Vec<Vec<u8>>,
    recs: Vec<Record>,
}

impl Store for MemStore {
    fn put(&mut self, _kind: MediaKind, bytes: &[u8]) -> Result<Handle, StoreError> {
        self.blobs.push(bytes.to_vec());
        Ok(Handle::new(format!("h{}", self.blobs.len())))
    }

    fn append_record(&mut self, rec: &Record) -> Result<(), StoreError> {
        self.recs.push(rec.clone());
        Ok(())
    }

    fn last_record_hash(&self, session: &SessionId) -> Option<[u8; 32]> {
        self.recs
            .iter()
            .rev()
            .find(|r| &r.session == session)
            .map(|r| r.hash)
    }
}

/// Deterministic “signature” = reversed hash (good enough for tests).
struct RevSigner;

impl Signer for RevSigner {
    fn sign(&self, msg: &[u8; 32]) -> Vec<u8> {
        msg.iter().rev().copied().collect()
    }
}

fn sig_ok(hash: &[u8; 32], sig: &[u8]) -> bool {
    sig.iter().eq(hash.iter().rev())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimal JPEG containing an APP1 segment and a trailer.
fn tiny_jpeg() -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x08];
    v.extend_from_slice(b"SECRET");
    v.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 1, 1, 0, 0, 0x3F, 0, 0xAA, 0xBB, 0xFF, 0xD9]);
    v.extend_from_slice(b"TRAILER");
    v
}

fn item(session: &str, kind: MediaKind) -> MediaItem {
    MediaItem {
        session: SessionId::new(session),
        kind,
        captured_ns: 1,
        bytes: tiny_jpeg(),
    }
}

fn marker(id: u64) -> Marker {
    Marker {
        id,
        t_ns: id,
        fix: FixQuality::Green,
        pos: Some((38.9, -77.0)),
    }
}

fn enabled_cfg() -> Config {
    Config {
        enabled: true,
        ..Config::default()
    }
}

fn make_ingestor(cfg: Config) -> Ingestor<MemStore, RevSigner, JpegScrubber> {
    Ingestor::new(cfg, MemStore::default(), RevSigner, JpegScrubber).unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn defaults_are_private_and_off() {
    let c = Config::default();
    assert!(!c.enabled);
    assert!(c.strip_metadata);
    assert!(!c.keep_originals);
    assert!(!c.record_position);
    assert!(!c.upload);

    let err = make_ingestor(Config::default())
        .ingest(item("s", MediaKind::Photo), &marker(1))
        .unwrap_err();
    assert!(matches!(err, IngestError::Disabled));
}

#[test]
fn chain_verifies_and_links() {
    let mut g = make_ingestor(enabled_cfg());
    for i in 1..=3 {
        g.ingest(item("s", MediaKind::Photo), &marker(i)).unwrap();
    }
    let recs = &g.store().recs;
    assert_eq!(recs[0].prev_hash, [0; 32]);
    assert_eq!(recs[1].prev_hash, recs[0].hash);
    assert_eq!(recs[2].prev_hash, recs[1].hash);
    assert_eq!(verify_chain(recs, sig_ok), Ok(()));
}

#[test]
fn tampering_is_detected() {
    let mut g = make_ingestor(enabled_cfg());
    for i in 1..=3 {
        g.ingest(item("s", MediaKind::Photo), &marker(i)).unwrap();
    }
    let good = g.store().recs.clone();

    // Corrupt content hash
    let mut a = good.clone();
    a[1].content_sha256[0] ^= 1;
    assert_eq!(verify_chain(&a, sig_ok), Err(ChainError::BadHash(1)));

    // Drop a record (breaks the link)
    let mut b = good.clone();
    b.remove(1);
    assert_eq!(verify_chain(&b, sig_ok), Err(ChainError::BrokenLink(1)));

    // Corrupt signature
    let mut c = good.clone();
    c[2].sig[0] ^= 1;
    assert_eq!(verify_chain(&c, sig_ok), Err(ChainError::BadSignature(2)));
}

#[test]
fn stored_media_has_no_exif_or_trailer() {
    let mut g = make_ingestor(enabled_cfg());
    g.ingest(item("s", MediaKind::Photo), &marker(1)).unwrap();
    let blob = &g.store().blobs[0];
    assert!(!blob.windows(6).any(|w| w == b"SECRET"));
    assert!(!blob.windows(7).any(|w| w == b"TRAILER"));
    assert_eq!(&blob[..2], &[0xFF, 0xD8]);
    assert_eq!(&blob[blob.len() - 2..], &[0xFF, 0xD9]);
}

#[test]
fn position_dropped_unless_enabled() {
    let mut g = make_ingestor(enabled_cfg());
    let rec = g.ingest(item("s", MediaKind::Photo), &marker(1)).unwrap();
    assert_eq!(rec.pos, None);

    let mut g = make_ingestor(Config {
        record_position: true,
        ..enabled_cfg()
    });
    let rec = g.ingest(item("s", MediaKind::Photo), &marker(1)).unwrap();
    assert_eq!(rec.pos, Some((38.9, -77.0)));
}

#[test]
fn fails_closed() {
    // Video is rejected by the scrubber
    let err = make_ingestor(enabled_cfg())
        .ingest(item("s", MediaKind::Video), &marker(1))
        .unwrap_err();
    assert!(matches!(err, IngestError::Scrub(_)));

    // Non-JPEG photo is rejected
    let mut junk = item("s", MediaKind::Photo);
    junk.bytes = b"not a jpeg".to_vec();
    let err = make_ingestor(enabled_cfg())
        .ingest(junk, &marker(1))
        .unwrap_err();
    assert!(matches!(err, IngestError::Scrub(_)));

    // Policy: unscrubbed without allow_unscrubbed
    assert!(Ingestor::new(
        Config {
            strip_metadata: false,
            ..enabled_cfg()
        },
        MemStore::default(),
        RevSigner,
        JpegScrubber,
    )
    .is_err());

    // Policy: upload not implemented
    assert!(Ingestor::new(
        Config {
            upload: true,
            ..enabled_cfg()
        },
        MemStore::default(),
        RevSigner,
        JpegScrubber,
    )
    .is_err());
}

#[test]
fn sessions_chain_independently() {
    let mut g = make_ingestor(enabled_cfg());
    g.ingest(item("a", MediaKind::Photo), &marker(1)).unwrap();
    let rb = g.ingest(item("b", MediaKind::Photo), &marker(2)).unwrap();
    // Session "b" starts a fresh chain
    assert_eq!(rb.prev_hash, [0; 32]);
}

#[test]
fn content_hash_matches_scrubbed_bytes() {
    use sha2::{Digest, Sha256};
    let mut g = make_ingestor(enabled_cfg());
    let rec = g.ingest(item("s", MediaKind::Photo), &marker(1)).unwrap();
    let expected: [u8; 32] = Sha256::digest(&g.store().blobs[0]).into();
    assert_eq!(rec.content_sha256, expected);
}

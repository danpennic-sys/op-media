//! Minimal example showing how to wire the traits and run an ingest.
//!
//! Run with: `cargo run --example basic`

use op_media::*;

// ---------------------------------------------------------------------------
// In-memory store (never use in production)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MemStore {
    blobs: Vec<Vec<u8>>,
    recs: Vec<Record>,
}

impl Store for MemStore {
    fn put(&mut self, _kind: MediaKind, bytes: &[u8]) -> Result<Handle, StoreError> {
        self.blobs.push(bytes.to_vec());
        Ok(Handle::new(format!("blob-{}", self.blobs.len())))
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

// ---------------------------------------------------------------------------
// Trivial signer for demonstration
// ---------------------------------------------------------------------------

struct DemoSigner;

impl Signer for DemoSigner {
    fn sign(&self, msg: &[u8; 32]) -> Vec<u8> {
        // In production replace with Ed25519 / hardware-backed signature
        msg.to_vec()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Tiny synthetic JPEG containing an APP1 segment that must be stripped
    let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x08];
    jpeg.extend_from_slice(b"SECRET");
    jpeg.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 1, 1, 0, 0, 0x3F, 0, 0xAA, 0xBB, 0xFF, 0xD9]);

    let cfg = Config {
        enabled: true,
        ..Config::default()
    };

    let mut ingestor = Ingestor::new(cfg, MemStore::default(), DemoSigner, JpegScrubber)?;

    let item = MediaItem {
        session: SessionId::new("demo-session"),
        kind: MediaKind::Photo,
        captured_ns: 1,
        bytes: jpeg,
    };

    let marker = Marker {
        id: 1,
        t_ns: 1,
        fix: FixQuality::Green,
        pos: Some((39.1, -94.6)), // will be dropped by default config
    };

    let record = ingestor.ingest(item, &marker)?;

    println!("Ingested record:");
    println!("  session      = {}", record.session);
    println!("  marker_id    = {}", record.marker_id);
    println!("  handle       = {}", record.handle.0);
    println!("  pos recorded = {:?}", record.pos);
    println!("  content_sha  = {}", bytes_to_hex(&record.content_sha256));
    println!("  hash         = {}", bytes_to_hex(&record.hash));
    println!("  prev_hash    = {}", bytes_to_hex(&record.prev_hash));

    // Verify the chain
    assert!(verify_chain(&ingestor.store().recs, |h, s| s == h).is_ok());
    println!("\nChain verification: OK");

    // Confirm the secret APP1 payload was removed
    let stored = &ingestor.store().blobs[0];
    assert!(!stored.windows(6).any(|w| w == b"SECRET"));
    println!("Metadata scrub: OK (SECRET segment removed)");

    Ok(())
}

fn bytes_to_hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

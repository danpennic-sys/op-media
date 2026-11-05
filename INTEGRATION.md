# Integrating op-media

This crate is designed to be called from a mobile operator app. The Rust
library owns the policy, scrubbing, hashing, and chaining logic. The host
(iOS / Android) owns everything that touches the OS, keystore, or network.

## Required host implementations

You must implement four traits and pass them into `Ingestor::new`:

| Trait        | Responsibility                                      | Typical backend                          |
|--------------|-----------------------------------------------------|------------------------------------------|
| `Store`      | Encrypted local write + durable record log          | Keychain / EncryptedSharedPreferences, SQLCipher, or Secure Enclave backed files |
| `Signer`     | Sign a 32-byte record hash                          | Secure Enclave (iOS), StrongBox / TEE (Android), or software Ed25519 |
| `Scrubber`   | Strip metadata (JPEG is provided; video is yours)   | Use the shipped `JpegScrubber`; add a video scrubber later |
| `Inference`  | Optional pose / detection (feature `pose`)          | MediaPipe, TFLite, Core ML               |

## Minimal wiring sketch (pseudo-Rust / FFI boundary)

```rust
// In the platform shell (or a thin FFI crate)
struct PlatformStore { /* file handles, DB connection */ }
impl Store for PlatformStore { /* … */ }

struct PlatformSigner { /* key handle */ }
impl Signer for PlatformSigner {
    fn sign(&self, msg: &[u8; 32]) -> Vec<u8> {
        // Call into Secure Enclave / Keystore
        hardware_sign(msg)
    }
}

// Construction (once per process or per session)
let cfg = Config {
    enabled: true,                 // turn the module on
    strip_metadata: true,
    record_position: false,        // keep location out of the chain by default
    ..Config::default()
};

let mut ingestor = Ingestor::new(
    cfg,
    PlatformStore::open()?,
    PlatformSigner::load()?,
    JpegScrubber,
)?;

// On each capture
let item = MediaItem {
    session: SessionId::new(current_session_id),
    kind: MediaKind::Photo,
    captured_ns: capture_timestamp_ns,
    bytes: jpeg_bytes_from_camera,
};
let marker = Marker {
    id: next_marker_id(),
    t_ns: item.captured_ns,
    fix: current_fix_quality(),
    pos: None,                     // or Some((lat, lon)) if record_position
};
let record = ingestor.ingest(item, &marker)?;
// `record` is now durable, signed, and chained. It never contains pixels.
```

## Fail-closed behaviour

- Module disabled → `IngestError::Disabled`
- Unknown / non-JPEG container → scrubber returns `Err` → ingest aborts
- `strip_metadata = false` without `allow_unscrubbed` → construction fails
- `upload = true` → construction fails (upload path not implemented here)

## Verifying a chain later

```rust
// Load the ordered records for a session from your Store
let recs: Vec<Record> = store.load_session(&session_id)?;

// Supply the same public-key verification the Signer used
let ok = verify_chain(&recs, |hash, sig| platform_verify(hash, sig));
```

## What stays out of this crate

- Camera / gallery access
- Network upload
- Actual encryption of the media blobs (the `Store::put` implementation owns that)
- Video container parsing
- UI

Keep those in the platform shell so the Rust core remains pure, auditable, and easy to test.

# op-media

Privacy-first, fail-closed media ingestion for operator applications.

The host application supplies only a `Marker` (id, timestamp, fix quality, optional position) and a session identifier. Platform shells implement the four traits (`Store`, `Signer`, `Scrubber`, `Inference`) via FFI; this crate never touches the OS.

## Design principles

- **Fail closed** – unknown formats, disabled module, or policy violations reject the item.
- **Metadata stripping** – JPEG scrubber removes EXIF/GPS/device tags and trailers by default.
- **No originals** – only scrubbed derivatives are stored unless explicitly configured otherwise.
- **Tamper-evident log** – every record is content-hashed, hash-chained to the previous record in the same session, and signed.
- **Minimal host surface** – the only required input from the core app is a `Marker`.

## Pipeline

```
scrub → SHA-256 → encrypted put → hash-chain (prev_hash) → sign → append
```

## Quick start

```rust
use op_media::*;

// 1. Provide platform adapters (shown here with test doubles)
struct MyStore { /* … */ }
impl Store for MyStore { /* … */ }

struct MySigner;
impl Signer for MySigner {
    fn sign(&self, msg: &[u8; 32]) -> Vec<u8> { /* Ed25519 / Secure Enclave … */ }
}

// 2. Configure (defaults are privacy-maximal and off)
let cfg = Config {
    enabled: true,
    strip_metadata: true,
    record_position: false,
    ..Config::default()
};

// 3. Build the ingestor
let mut ingestor = Ingestor::new(cfg, MyStore::new(), MySigner, JpegScrubber)?;

// 4. Ingest
let item = MediaItem {
    session: SessionId::new("mission-42"),
    kind: MediaKind::Photo,
    captured_ns: 1_700_000_000_000_000_000,
    bytes: jpeg_bytes,
};
let marker = Marker {
    id: 7,
    t_ns: item.captured_ns,
    fix: FixQuality::Green,
    pos: None,
};
let record = ingestor.ingest(item, &marker)?;
```

## Configuration defaults

| Flag               | Default | Meaning                                      |
|--------------------|---------|----------------------------------------------|
| `enabled`          | `false` | Master switch                                |
| `strip_metadata`   | `true`  | Run scrubber before storage                  |
| `allow_unscrubbed` | `false` | Permit `strip_metadata = false`              |
| `keep_originals`   | `false` | Keep unscrubbed bytes (not used by pipeline) |
| `record_position`  | `false` | Write `Marker::pos` into the chain           |
| `upload`           | `false` | Network upload (rejected in this crate)      |

## What is implemented

- JPEG scrubber (APP1–13/15 + COM dropped, trailing data after EOI cut)
- Full ingest pipeline with policy enforcement
- Domain-separated, length-prefixed record hashing
- Chain verification (linkage + content hash + signature)
- Comprehensive integration tests

## Not implemented (intentionally left to the platform shell)

- Video container scrubbing
- Real encrypted / hardware-backed `Store`
- Cryptographic `Signer` (Secure Enclave, StrongBox, …)
- Inference / MediaPipe adapter
- Network upload / watch-marker transport

## License

MIT OR Apache-2.0

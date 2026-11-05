//! Metadata scrubbers.
//!
//! JPEG is fully supported. All other formats (including video) fail closed
//! so that an unknown container can never leak location or device data.

use crate::types::{MediaKind, ScrubError};
use crate::Scrubber;

/// Production JPEG scrubber used by the default pipeline.
#[derive(Debug, Default, Clone, Copy)]
pub struct JpegScrubber;

impl Scrubber for JpegScrubber {
    fn scrub(&self, kind: MediaKind, bytes: &[u8]) -> Result<Vec<u8>, ScrubError> {
        match kind {
            MediaKind::Photo => strip_jpeg(bytes),
            MediaKind::Video => Err(ScrubError::new(
                "video scrubbing not implemented (fail closed)",
            )),
        }
    }
}

/// Keep only decode-relevant JPEG segments.
///
/// - Drops APP1–APP13, APP15, COM (EXIF, XMP, ICC, IPTC, comments, …)
/// - Keeps APP0 (JFIF) and APP14 (Adobe colour transform)
/// - Truncates anything after the first EOI (trailers, MPF, hidden thumbnails)
fn strip_jpeg(b: &[u8]) -> Result<Vec<u8>, ScrubError> {
    let err = |m: &str| ScrubError::new(m);

    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 {
        return Err(err("not a JPEG"));
    }

    let mut out = vec![0xFF, 0xD8];
    let mut i = 2usize;

    loop {
        if i + 1 >= b.len() {
            return Err(err("truncated before SOS"));
        }
        if b[i] != 0xFF {
            return Err(err("bad marker"));
        }

        let marker = b[i + 1];
        match marker {
            // Padding / fill bytes
            0xFF => {
                i += 1;
            }
            // Stand-alone markers (no length field)
            0x01 | 0xD0..=0xD8 => {
                out.extend_from_slice(&b[i..i + 2]);
                i += 2;
            }
            // EOI before we have seen SOS is invalid
            0xD9 => return Err(err("EOI before SOS")),
            // All other markers have a 2-byte length field
            _ => {
                if i + 4 > b.len() {
                    return Err(err("truncated segment header"));
                }
                let len = ((b[i + 2] as usize) << 8) | (b[i + 3] as usize);
                let end = i + 2 + len;
                if len < 2 || end > b.len() {
                    return Err(err("bad segment length"));
                }

                // Drop privacy-sensitive application segments and comments
                let drop = matches!(marker, 0xE1..=0xED | 0xEF | 0xFE);
                if !drop {
                    out.extend_from_slice(&b[i..end]);
                }

                // Start-of-Scan: copy entropy-coded data up to (and including) EOI
                if marker == 0xDA {
                    let eoi = (end..b.len().saturating_sub(1))
                        .find(|&j| b[j] == 0xFF && b[j + 1] == 0xD9)
                        .ok_or_else(|| err("missing EOI"))?;
                    out.extend_from_slice(&b[end..eoi + 2]);
                    return Ok(out);
                }

                i = end;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_jpeg_with_secret() -> Vec<u8> {
        // SOI, APP1("SECRET"), SOS header, 2 entropy bytes, EOI, trailer
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x08];
        v.extend_from_slice(b"SECRET");
        v.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 1, 1, 0, 0, 0x3F, 0, 0xAA, 0xBB, 0xFF, 0xD9]);
        v.extend_from_slice(b"TRAILER");
        v
    }

    #[test]
    fn strips_app1_and_trailer() {
        let scrubber = JpegScrubber;
        let clean = scrubber.scrub(MediaKind::Photo, &tiny_jpeg_with_secret()).unwrap();
        assert!(!clean.windows(6).any(|w| w == b"SECRET"));
        assert!(!clean.windows(7).any(|w| w == b"TRAILER"));
        assert_eq!(&clean[..2], &[0xFF, 0xD8]);
        assert_eq!(&clean[clean.len() - 2..], &[0xFF, 0xD9]);
    }

    #[test]
    fn rejects_non_jpeg() {
        let scrubber = JpegScrubber;
        assert!(scrubber.scrub(MediaKind::Photo, b"not jpeg").is_err());
    }

    #[test]
    fn rejects_video() {
        let scrubber = JpegScrubber;
        assert!(scrubber.scrub(MediaKind::Video, &[0u8; 16]).is_err());
    }
}

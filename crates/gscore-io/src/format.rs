//! Detecting a Guitar Pro file's format from its first bytes (never from the file name:
//! real-world files often carry the wrong extension).

use std::fmt;

use crate::error::IoError;

/// Header of GP3/GP4/GP5 files, after a one-byte length prefix.
const LEGACY_MAGIC: &[u8] = b"FICHIER GUITAR PRO v";
/// GP7+ files are zip archives.
const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
/// GP6 files are BCFZ (compressed) or BCFS containers.
const GPX_MAGICS: [&[u8]; 2] = [b"BCFZ", b"BCFS"];

/// A Guitar Pro file format that can be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileFormat {
    /// Guitar Pro 3 (`.gp3`).
    Gp3,
    /// Guitar Pro 4 (`.gp4`).
    Gp4,
    /// Guitar Pro 5 (`.gp5`).
    Gp5,
    /// Guitar Pro 6 (`.gpx`).
    Gpx,
    /// Guitar Pro 7 and later (`.gp`).
    Gp,
}

impl FileFormat {
    /// The usual file extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            FileFormat::Gp3 => "gp3",
            FileFormat::Gp4 => "gp4",
            FileFormat::Gp5 => "gp5",
            FileFormat::Gpx => "gpx",
            FileFormat::Gp => "gp",
        }
    }
}

impl fmt::Display for FileFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            FileFormat::Gp3 => "Guitar Pro 3",
            FileFormat::Gp4 => "Guitar Pro 4",
            FileFormat::Gp5 => "Guitar Pro 5",
            FileFormat::Gpx => "Guitar Pro 6",
            FileFormat::Gp => "Guitar Pro 7+",
        };
        write!(f, "{name} (.{})", self.extension())
    }
}

/// Determines the format from the file's first bytes.
pub fn detect_format(bytes: &[u8]) -> Result<FileFormat, IoError> {
    if bytes.starts_with(ZIP_MAGIC) {
        return Ok(FileFormat::Gp);
    }
    if GPX_MAGICS.iter().any(|magic| bytes.starts_with(magic)) {
        return Ok(FileFormat::Gpx);
    }
    // GP3-5: one length byte, then "FICHIER GUITAR PRO v<major>.<minor>".
    if let Some(header) = bytes.get(1..) {
        if let Some(version) = header.strip_prefix(LEGACY_MAGIC) {
            return match version.first() {
                Some(b'3') => Ok(FileFormat::Gp3),
                Some(b'4') => Ok(FileFormat::Gp4),
                Some(b'5') => Ok(FileFormat::Gp5),
                _ => Err(IoError::UnsupportedFormat(format!(
                    "Guitar Pro version {}",
                    String::from_utf8_lossy(version.get(..4).unwrap_or(version))
                ))),
            };
        }
    }
    Err(IoError::UnsupportedFormat(
        "not a Guitar Pro file (unknown header); supported: .gp3, .gp4, .gp5, .gpx, .gp".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy(version: &str) -> Vec<u8> {
        let text = format!("FICHIER GUITAR PRO {version}");
        let mut bytes = vec![u8::try_from(text.len()).unwrap()];
        bytes.extend_from_slice(text.as_bytes());
        bytes.extend_from_slice(&[0; 16]);
        bytes
    }

    #[test]
    fn detects_every_supported_format() {
        let cases = [
            (legacy("v3.00"), FileFormat::Gp3),
            (legacy("v4.06"), FileFormat::Gp4),
            (legacy("v5.10"), FileFormat::Gp5),
            (b"BCFZ\x00\x01".to_vec(), FileFormat::Gpx),
            (b"BCFS\x00\x01".to_vec(), FileFormat::Gpx),
            (b"PK\x03\x04\x14\x00".to_vec(), FileFormat::Gp),
        ];
        for (bytes, expected) in cases {
            assert_eq!(detect_format(&bytes).unwrap(), expected);
        }
    }

    #[test]
    fn rejects_unknown_and_unsupported_headers() {
        for bytes in [
            &b""[..],
            b"P",
            b"\x00",
            b"hello world",
            &legacy("v2.21"),
            &legacy("v"),
        ] {
            assert!(
                matches!(detect_format(bytes), Err(IoError::UnsupportedFormat(_))),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn formats_display_name_and_extension() {
        assert_eq!(FileFormat::Gp.to_string(), "Guitar Pro 7+ (.gp)");
        assert_eq!(FileFormat::Gp5.extension(), "gp5");
    }
}

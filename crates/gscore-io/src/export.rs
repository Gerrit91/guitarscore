//! Export of a `Song` to Guitar Pro bytes.

use gscore_model::Song;

use crate::error::IoError;

/// Serializes a song to Guitar Pro bytes.
///
/// Not implemented yet (milestone M6). It returns an error rather than writing a partial
/// file, because a file that silently drops the user's music is data loss.
pub fn export_guitar_pro_to_bytes(_song: &Song) -> Result<Vec<u8>, IoError> {
    Err(IoError::Unsupported(
        "saving Guitar Pro files (planned for M6)",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_is_reported_as_unsupported() {
        let song = Song::default();
        assert!(matches!(
            export_guitar_pro_to_bytes(&song),
            Err(IoError::Unsupported(_))
        ));
    }
}

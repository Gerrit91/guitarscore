//! The file names of a stem export: one per track, each distinct, none taken by a bus.

/// The names the bus stems take ("<song> - Reverb.wav", ...).
pub const BUS_STEM_NAMES: [&str; 3] = ["Reverb", "Compressor", "Chorus"];

/// A file name for each of `tracks` (their names, in song order; an empty one becomes
/// "Track N"), which a file system can hold. Two tracks never share a name, and no track
/// takes a bus stem's: a later duplicate gets its track number ("Guitar (track 3)").
/// Names are compared without case, as some file systems do.
pub fn stem_names(tracks: &[String]) -> Vec<String> {
    let mut used: Vec<String> = BUS_STEM_NAMES
        .iter()
        .map(|name| name.to_lowercase())
        .collect();
    let mut names = Vec::with_capacity(tracks.len());
    for (index, name) in tracks.iter().enumerate() {
        let number = index.saturating_add(1);
        let trimmed = name.trim();
        let base = if trimmed.is_empty() {
            format!("Track {number}")
        } else {
            file_name(trimmed)
        };
        let mut chosen = base.clone();
        let mut attempt = 1_usize;
        while used.contains(&chosen.to_lowercase()) {
            chosen = if attempt == 1 {
                format!("{base} (track {number})")
            } else {
                format!("{base} (track {number}, {attempt})")
            };
            attempt = attempt.saturating_add(1);
        }
        used.push(chosen.to_lowercase());
        names.push(chosen);
    }
    names
}

/// A name a file system can hold (no path separators or control characters), or "Track".
fn file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_owned();
    if cleaned.is_empty() {
        "Track".to_owned()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(tracks: &[&str]) -> Vec<String> {
        stem_names(
            &tracks
                .iter()
                .map(|&name| name.to_owned())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn two_tracks_with_one_name_get_two_files() {
        assert_eq!(
            names(&["Guitar", "Bass", "guitar"]),
            ["Guitar", "Bass", "guitar (track 3)"]
        );
    }

    #[test]
    fn a_track_named_like_a_bus_does_not_take_its_file() {
        assert_eq!(
            names(&["Reverb", "Chorus"]),
            ["Reverb (track 1)", "Chorus (track 2)"]
        );
    }

    #[test]
    fn an_empty_or_odd_name_still_gives_a_file() {
        assert_eq!(names(&["", "a/b", "..."]), ["Track 1", "a_b", "Track"]);
    }

    #[test]
    fn a_name_that_happens_to_be_a_suffixed_one_is_still_distinct() {
        assert_eq!(
            names(&["Gtr", "Gtr (track 3)", "Gtr"]),
            ["Gtr", "Gtr (track 3)", "Gtr (track 3, 2)"]
        );
    }
}

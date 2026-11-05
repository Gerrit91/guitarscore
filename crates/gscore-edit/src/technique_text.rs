//! The text form of the effects that carry more than a switch or a kind: trills
//! (`4:16`, `auto:32`, `none`), tremolo picking speeds (`8`, `16`, `32`, `none`) and
//! fingering (`left:1`, `right:i`, `left:none`). `true` and `false` still read as before
//! (a trill two frets up, sixteenths), so older recovery journals replay.

use gscore_model::{Finger, Hand, TremoloSpeed, Trill, TrillSpeed};

/// A trill as `fret:speed` (`auto` for two frets above the note), or `none`.
pub(crate) fn trill_text(trill: Option<Trill>) -> String {
    match trill {
        None => "none".to_owned(),
        Some(trill) => {
            let fret = trill
                .fret
                .map_or_else(|| "auto".to_owned(), |fret| fret.to_string());
            format!("{fret}:{}", trill.speed.note_value())
        }
    }
}

/// Reads [`trill_text`]'s form.
pub(crate) fn parse_trill(value: &str) -> Result<Option<Trill>, String> {
    const FORM: &str = "trill: fret:16|32|64 (fret or auto), true, false or none";
    match value {
        "none" | "false" => return Ok(None),
        "true" => {
            return Ok(Some(Trill {
                fret: None,
                speed: TrillSpeed::Sixteenth,
            }));
        }
        _ => {}
    }
    let (fret, speed) = value.split_once(':').ok_or_else(|| FORM.to_owned())?;
    let fret = match fret {
        "auto" => None,
        fret => Some(fret.parse::<u8>().map_err(|_| FORM.to_owned())?),
    };
    let speed = TrillSpeed::ALL
        .into_iter()
        .find(|s| s.note_value().to_string() == speed)
        .ok_or_else(|| FORM.to_owned())?;
    Ok(Some(Trill { fret, speed }))
}

/// A tremolo picking speed as its note value, or `none`.
pub(crate) fn tremolo_text(speed: Option<TremoloSpeed>) -> String {
    speed.map_or_else(|| "none".to_owned(), |s| s.note_value().to_string())
}

/// Reads [`tremolo_text`]'s form.
pub(crate) fn parse_tremolo(value: &str) -> Result<Option<TremoloSpeed>, String> {
    match value {
        "none" | "false" => Ok(None),
        "true" => Ok(Some(TremoloSpeed::Sixteenth)),
        _ => TremoloSpeed::ALL
            .into_iter()
            .find(|s| s.note_value().to_string() == value)
            .map(Some)
            .ok_or_else(|| "tremolo_picking: 8, 16, 32 or none".to_owned()),
    }
}

/// A fingering as `hand:finger` (the finger as written for that hand), or `hand:none`.
pub(crate) fn finger_text(hand: Hand, finger: Option<Finger>) -> String {
    let hand_id = match hand {
        Hand::Left => "left",
        Hand::Right => "right",
    };
    let finger = finger.map_or("none", |finger| match finger {
        Finger::Open => "0",
        other => other.sign(hand),
    });
    format!("{hand_id}:{finger}")
}

/// Reads [`finger_text`]'s form.
pub(crate) fn parse_finger(value: &str) -> Result<(Hand, Option<Finger>), String> {
    const FORM: &str = "finger: left:0|1|2|3|4|t|none or right:p|i|m|a|c|none";
    let (hand, finger) = value.split_once(':').ok_or_else(|| FORM.to_owned())?;
    let hand = match hand {
        "left" => Hand::Left,
        "right" => Hand::Right,
        _ => return Err(FORM.to_owned()),
    };
    if finger == "none" {
        return Ok((hand, None));
    }
    let finger = Finger::ALL
        .into_iter()
        .find(|f| f.sign(hand) == finger)
        .ok_or_else(|| FORM.to_owned())?;
    Ok((hand, Some(finger)))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn the_forms_read_back() {
        for trill in [
            None,
            Some(Trill {
                fret: Some(4),
                speed: TrillSpeed::ThirtySecond,
            }),
            Some(Trill {
                fret: None,
                speed: TrillSpeed::SixtyFourth,
            }),
        ] {
            assert_eq!(parse_trill(&trill_text(trill)).unwrap(), trill);
        }
        for speed in [
            None,
            Some(TremoloSpeed::Eighth),
            Some(TremoloSpeed::ThirtySecond),
        ] {
            assert_eq!(parse_tremolo(&tremolo_text(speed)).unwrap(), speed);
        }
        for hand in [Hand::Left, Hand::Right] {
            for finger in Finger::ALL.map(Some).into_iter().chain([None]) {
                let text = finger_text(hand, finger);
                assert_eq!(parse_finger(&text).unwrap(), (hand, finger), "{text}");
            }
        }
        assert_eq!(
            parse_trill("true").unwrap().unwrap().fret,
            None,
            "old journals"
        );
        assert_eq!(
            parse_tremolo("true").unwrap(),
            Some(TremoloSpeed::Sixteenth)
        );
        assert!(parse_finger("left:x").is_err() && parse_trill("4").is_err());
    }
}

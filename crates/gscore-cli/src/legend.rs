//! `legend`: the symbols `gscore-cli dump` writes in its ASCII tablature.

use crate::dump::{Annotation, Symbol};

/// The legend text. It is generated from the same enums the dump uses, so a new symbol
/// cannot end up in the tablature without appearing here.
pub(crate) fn run() -> String {
    let mut out =
        String::from("ASCII tablature symbols used by `gscore-cli dump`\n\nAfter a fret or beat\n");
    for symbol in Symbol::ALL {
        out.push_str(&format!(
            "  {:<6} {}\n",
            symbol.text(),
            symbol.description()
        ));
    }
    out.push_str("\nRows under the staff\n");
    for &annotation in Annotation::ALL {
        out.push_str(&format!(
            "  {:<6} {} (its mark starts a run, `-` continues it, `|` ends it)\n",
            annotation.mark(),
            annotation.description()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_symbol_has_a_legend_line() {
        let text = run();
        for symbol in Symbol::ALL {
            assert!(text.contains(symbol.text()), "{text}");
            assert!(text.contains(symbol.description()), "{text}");
        }
        for &annotation in Annotation::ALL {
            assert!(text.contains(annotation.mark()), "{text}");
            assert!(text.contains(annotation.description()), "{text}");
        }
    }
}

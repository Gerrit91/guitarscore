//! GuitarScore regression tests for section markers (see docs/guitarpro.md).

use crate::io::Song;
use crate::io::gpif_import::SongGpifOps;
use crate::model::Marker;

/// Reads the section titles and colors of a GPIF document with the given master bars.
fn sections(master_bars: &str) -> Vec<Option<(String, i32)>> {
    let xml = format!(
        "<GPIF><Score></Score><MasterTrack></MasterTrack><Tracks><Track id=\"0\">\
         <Name>Guitar</Name></Track></Tracks><MasterBars>{master_bars}</MasterBars>\
         <Bars></Bars><Voices></Voices><Beats></Beats><Notes></Notes>\
         <Rhythms></Rhythms></GPIF>"
    );
    let mut song = Song::default();
    song.read_gpif(&quick_xml::de::from_str(&xml).expect("test GPIF parses"));
    song.measure_headers
        .iter()
        .map(|header| {
            header
                .marker
                .as_ref()
                .map(|marker| (marker.title.clone(), marker.color))
        })
        .collect()
}

#[test]
fn section_text_loses_the_cdata_formatting_whitespace() {
    // Guitar Pro writes the text in a CDATA block between newlines, as in a real file.
    let bars = "<MasterBar><Time>4/4</Time><Section>\
                <Letter>\n<![CDATA[]]>\n</Letter>\
                <Text>\n<![CDATA[ Chorus 1 ]]>\n</Text>\
                </Section></MasterBar>";
    assert_eq!(
        sections(bars),
        vec![Some(("Chorus 1".to_owned(), Marker::DEFAULT_COLOR))]
    );
}

#[test]
fn a_section_without_text_falls_back_to_its_letter() {
    let bars = "<MasterBar><Time>4/4</Time><Section>\
                <Letter>\n<![CDATA[B]]>\n</Letter>\
                <Text>\n<![CDATA[   ]]>\n</Text>\
                </Section></MasterBar>";
    assert_eq!(
        sections(bars),
        vec![Some(("B".to_owned(), Marker::DEFAULT_COLOR))]
    );
}

#[test]
fn a_section_with_neither_text_nor_letter_keeps_the_placeholder() {
    let bars = "<MasterBar><Time>4/4</Time><Section>\
                <Letter>\n<![CDATA[]]>\n</Letter>\
                <Text>\n<![CDATA[]]>\n</Text>\
                </Section></MasterBar>";
    assert_eq!(
        sections(bars),
        vec![Some(("Section".to_owned(), Marker::DEFAULT_COLOR))]
    );
}

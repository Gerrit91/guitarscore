//! GuitarScore regression tests for GP6/GP7 beat lyrics (see docs/guitarpro.md).

use crate::io::Song;
use crate::io::gpif_import::SongGpifOps;

/// Two tracks of one bar; the second track's beat carries `lyrics` (its `<Lyrics>` element).
fn song_with_beat_lyrics(lyrics: &str) -> Song {
    let xml = format!(
        "<GPIF><Score></Score><MasterTrack></MasterTrack><Tracks>\
         <Track id=\"0\"><Name>Guitar</Name></Track>\
         <Track id=\"1\"><Name>Vocals</Name>\
         <Lyrics dispatched=\"true\"><Line><Text><![CDATA[]]></Text><Offset>0</Offset></Line></Lyrics>\
         </Track></Tracks>\
         <MasterBars><MasterBar><Time>4/4</Time><Bars>0 1</Bars></MasterBar></MasterBars>\
         <Bars><Bar id=\"0\"><Voices>0 -1 -1 -1</Voices></Bar>\
         <Bar id=\"1\"><Voices>1 -1 -1 -1</Voices></Bar></Bars>\
         <Voices><Voice id=\"0\"><Beats>0</Beats></Voice><Voice id=\"1\"><Beats>1</Beats></Voice></Voices>\
         <Beats><Beat id=\"0\"><Rhythm ref=\"0\"/></Beat>\
         <Beat id=\"1\"><Rhythm ref=\"0\"/>{lyrics}</Beat></Beats>\
         <Notes></Notes><Rhythms><Rhythm id=\"0\"><NoteValue>Whole</NoteValue></Rhythm></Rhythms></GPIF>"
    );
    let mut song = Song::default();
    song.read_gpif(&quick_xml::de::from_str(&xml).expect("test GPIF parses"));
    song
}

fn lyrics_of_track(song: &Song, track: usize) -> Vec<String> {
    song.tracks[track].measures[0].voices[0].beats[0]
        .lyrics
        .clone()
}

#[test]
fn guitar_pro_beat_lyrics_are_read_line_by_line() {
    // As Guitar Pro 7 writes them: five `<Line>`s in CDATA, most of them empty.
    let song = song_with_beat_lyrics(
        "<Lyrics><Line><![CDATA[So]]></Line><Line><![CDATA[]]></Line>\
         <Line><![CDATA[too]]></Line><Line><![CDATA[]]></Line><Line><![CDATA[]]></Line></Lyrics>",
    );
    assert_eq!(lyrics_of_track(&song, 1), ["So", "", "too"]);
    assert!(lyrics_of_track(&song, 0).is_empty());
    // The track whose beats hold the lyrics becomes the lyric track.
    assert_eq!(song.lyrics.track_choice, 1);
}

#[test]
fn beat_lyrics_are_written_as_guitar_pro_writes_them() {
    let mut song = song_with_beat_lyrics(
        "<Lyrics><Line><![CDATA[it's]]></Line><Line><![CDATA[]]></Line></Lyrics>",
    );
    song.tracks[1].measures[0].voices[0].beats[0].lyrics = vec!["a<b".to_owned(), "two".to_owned()];
    let bytes = song.write_gp().expect("song writes");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("written file reads");
    assert_eq!(lyrics_of_track(&reread, 1), ["a<b", "two"]);
}

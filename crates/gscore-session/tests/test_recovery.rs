//! The recovery journal: what is recorded comes back after a crash, with its undo history;
//! damaged and missing pieces are survived; a killed process is recovered (AGENTS.md
//! "Recovery test").

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::time::{Duration, Instant};

use gscore_edit::{Command, TabEditor};
use gscore_model::{
    BeatIndex, BeatPosition, Fret, MeasureIndex, Song, StringNumber, TrackIndex, VoiceIndex,
    differences,
};
use gscore_session::{FoundSession, Journal, discard, find_sessions, recover};

/// Set in the child process of the kill test: its recovery directory.
const CHILD_ENV: &str = "GSCORE_RECOVERY_CHILD";

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gscore-recovery-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn base_bytes() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/basic/note_lengths.gp");
    std::fs::read(path).unwrap()
}

fn base_song() -> Song {
    gscore_io::import_guitar_pro_from_bytes(&base_bytes())
        .unwrap()
        .song
}

/// The `i`th edit of a deterministic sequence: a fret on the first beat of a measure.
fn nth_edit(i: usize, measures: usize) -> Command {
    Command::SetFret {
        at: BeatPosition {
            staff: gscore_model::StaffIndex::new(0),
            track: TrackIndex::new(0),
            measure: MeasureIndex::new(i % measures),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(0),
        },
        string: StringNumber::new(u8::try_from(i % 6 + 1).unwrap()).unwrap(),
        fret: Fret::new(u8::try_from(i % 20).unwrap()).unwrap(),
    }
}

/// Edits, undoes and redoes on a fresh editor, recording into `journal`.
fn edit(editor: &mut TabEditor, journal: &mut Journal, count: usize) {
    let measures = editor.song().measure_count();
    for i in 0..count {
        editor.replay(nth_edit(i, measures)).unwrap();
        if i % 7 == 6 {
            editor.undo().unwrap();
            if i % 14 == 13 {
                editor.redo().unwrap();
            }
        }
        let applied = editor.take_applied();
        journal.record(&applied, editor.song());
    }
}

fn only_session(root: &Path) -> FoundSession {
    let found = find_sessions(root);
    assert_eq!(found.len(), 1, "{found:?}");
    found.into_iter().next().unwrap()
}

/// Both editors hold the same song now and after every undo.
fn assert_same_history(mut expected: TabEditor, mut recovered: TabEditor) {
    assert!(differences(expected.song(), recovered.song()).is_empty());
    while expected.can_undo() {
        expected.undo().unwrap();
        recovered.undo().unwrap();
        assert!(differences(expected.song(), recovered.song()).is_empty());
    }
    assert!(!recovered.can_undo());
}

#[test]
fn a_crashed_document_comes_back_with_its_undo_history() {
    let root = temp_dir("crash");
    let source = root.join("song.gp");
    let mut journal = Journal::start(&root, Some(&source), &base_bytes()).unwrap();
    let mut editor = TabEditor::new(base_song(), TrackIndex::new(0));
    edit(&mut editor, &mut journal, 40);
    // While the document is open, no other window may take it.
    assert!(
        find_sessions(&root).is_empty(),
        "a live journal is not offered"
    );
    drop(journal); // as a crash would leave it (without the normal close)

    let session = only_session(&root);
    assert_eq!(session.source.as_deref(), Some(source.as_path()));
    assert!(session.edits >= 40);
    let recovered = recover(&session).unwrap();
    assert_eq!(recovered.format, gscore_io::FileFormat::Gp);
    assert!(!recovered.from_snapshot);
    assert_eq!(recovered.lost, 0);
    assert_eq!(recovered.base, base_bytes());
    assert_same_history(editor, recovered.editor);

    discard(&session).unwrap();
    assert!(find_sessions(&root).is_empty());
    assert!(!session.dir.exists());
}

#[test]
fn closed_and_saved_documents_leave_nothing_to_recover() {
    let root = temp_dir("closed");
    // Closed normally: the folder goes.
    let mut journal = Journal::start(&root, None, &base_bytes()).unwrap();
    let mut editor = TabEditor::new(base_song(), TrackIndex::new(0));
    edit(&mut editor, &mut journal, 5);
    let dir = journal.dir().to_path_buf();
    journal.close();
    assert!(!dir.exists());

    // Saved, then the process ended: nothing after the save, and the leftover is cleaned
    // up when looking.
    let mut journal = Journal::start(&root, None, &base_bytes()).unwrap();
    let mut editor = TabEditor::new(base_song(), TrackIndex::new(0));
    edit(&mut editor, &mut journal, 5);
    let saved_to = root.join("saved.gp");
    journal.saved(&saved_to);
    let dir = journal.dir().to_path_buf();
    drop(journal);
    assert!(find_sessions(&root).is_empty());
    assert!(!dir.exists(), "a folder with nothing to recover is removed");

    // Saved, then edited: only the edits since the save count, and the new path is kept.
    let mut journal = Journal::start(&root, None, &base_bytes()).unwrap();
    let mut editor = TabEditor::new(base_song(), TrackIndex::new(0));
    edit(&mut editor, &mut journal, 5);
    journal.saved(&saved_to);
    let measures = editor.song().measure_count();
    editor.replay(nth_edit(9, measures)).unwrap();
    let applied = editor.take_applied();
    journal.record(&applied, editor.song());
    drop(journal);
    let session = only_session(&root);
    assert_eq!(session.edits, 1);
    assert_eq!(session.source.as_deref(), Some(saved_to.as_path()));
    assert_same_history(editor, recover(&session).unwrap().editor);
}

#[test]
fn a_damaged_journal_end_and_a_lost_base_are_survived() {
    let root = temp_dir("damaged");
    let mut journal = Journal::start(&root, None, &base_bytes()).unwrap();
    let mut editor = TabEditor::new(base_song(), TrackIndex::new(0));
    edit(&mut editor, &mut journal, 10);
    journal.snapshot(editor.song());
    let at_snapshot = editor.song().clone();
    edit_more(&mut editor, &mut journal);
    let dir = journal.dir().to_path_buf();
    drop(journal);

    // A crash in the middle of the last line: everything before it is recovered.
    let journal_path = dir.join("journal");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&journal_path)
        .unwrap();
    file.write_all(b"0123456789abcdef set_fret meas").unwrap();
    drop(file);
    let session = only_session(&root);
    let recovered = recover(&session).unwrap();
    assert_eq!(recovered.lost, 1);
    assert!(differences(editor.song(), recovered.editor.song()).is_empty());

    // The base is gone (a disk problem): the snapshot and the entries after it remain.
    std::fs::write(dir.join("base"), b"not a song").unwrap();
    let recovered = recover(&session).unwrap();
    assert!(recovered.from_snapshot);
    assert!(differences(editor.song(), recovered.editor.song()).is_empty());
    let mut snapshot_editor = recovered.editor;
    while snapshot_editor.can_undo() {
        snapshot_editor.undo().unwrap();
    }
    assert!(
        differences(&at_snapshot, snapshot_editor.song()).is_empty(),
        "undo goes back to the snapshot, not before"
    );

    // Neither base nor snapshot: an error, not a panic.
    std::fs::remove_file(dir.join("snapshot.gp")).unwrap();
    assert!(recover(&session).is_err());
}

fn edit_more(editor: &mut TabEditor, journal: &mut Journal) {
    let measures = editor.song().measure_count();
    for i in 20..26 {
        editor.replay(nth_edit(i, measures)).unwrap();
        let applied = editor.take_applied();
        journal.record(&applied, editor.song());
    }
}

/// The child half of the kill test: edits and journals until it is killed. Does nothing
/// unless started by `a_killed_process_is_recovered`.
#[test]
fn child_edits_until_killed() {
    let Some(root) = std::env::var_os(CHILD_ENV) else {
        return;
    };
    let mut journal = Journal::start(Path::new(&root), None, &base_bytes()).unwrap();
    let mut editor = TabEditor::new(base_song(), TrackIndex::new(0));
    let measures = editor.song().measure_count();
    let mut out = std::io::stdout();
    for i in 0..100_000 {
        editor.replay(nth_edit(i, measures)).unwrap();
        let applied = editor.take_applied();
        journal.record(&applied, editor.song());
        // Report what is on disk, which is what a crash must not lose.
        writeln!(out, "edits {}", journal.written()).unwrap();
        out.flush().unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    // Never closed normally: the parent kills this process first.
    std::process::abort();
}

#[test]
fn a_killed_process_is_recovered() {
    let root = temp_dir("killed");
    let mut child = Process::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_edits_until_killed",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, &root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let started = Instant::now();
    let mut reported = 0;
    while reported < 60 {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "the child is too slow"
        );
        let Some(Ok(line)) = lines.next() else {
            panic!("the child ended early");
        };
        if let Some(count) = line.strip_prefix("edits ") {
            reported = count.parse().unwrap();
        }
    }
    child.kill().unwrap(); // SIGKILL: no cleanup runs
    let _ = child.wait();

    let session = only_session(&root);
    let recovered = recover(&session).unwrap();
    // Everything the child reported as written to disk comes back (the writer may have
    // written a little more before the kill).
    assert!(
        recovered.replayed >= reported,
        "{} of {reported} edits recovered",
        recovered.replayed
    );
    let mut expected = TabEditor::new(base_song(), TrackIndex::new(0));
    let measures = expected.song().measure_count();
    for i in 0..recovered.replayed {
        expected.replay(nth_edit(i, measures)).unwrap();
    }
    assert_same_history(expected, recovered.editor);
}

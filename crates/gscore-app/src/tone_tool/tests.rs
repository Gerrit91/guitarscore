//! Tests for the tone import tool.

use crate::chain_settings::{BlockSetting, Tone};
use crate::settings::kind_from_id;
use crate::tone_library;

use super::import::{drop_impulse_responses, run_import};
use super::normalize::run_normalize;
use super::rse::{RseArgs, run_rse};
use super::update::run_update;
use super::*;

fn library() -> ToneLibraryFile {
    serde_json::from_str(tone_library::TONES_JSON).unwrap()
}

fn preset(name: &str, kind: &str, level: f32, blocks: Vec<BlockSetting>) -> ChainPreset {
    ChainPreset {
        name: name.to_owned(),
        kind: kind.to_owned(),
        sound: crate::settings::StringSound {
            level,
            ..crate::settings::StringSound::default()
        },
        chain: ChainSetting {
            blocks: {
                let mut array = std::array::from_fn(|_| BlockSetting::default());
                for (slot, block) in array.iter_mut().zip(blocks) {
                    *slot = block;
                }
                array
            },
            output_db: 0.0,
        },
        engine: String::new(),
    }
}

/// A fresh directory for one test (the process id keeps parallel runs apart).
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gscore-tones-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A settings file with `presets` (everything else stays at its default).
fn write_settings(dir: &Path, presets: &[ChainPreset]) -> PathBuf {
    let path = dir.join("settings.json");
    let text = serde_json::json!({ "string_presets": presets }).to_string();
    std::fs::write(&path, text).unwrap();
    path
}

/// The embedded library, copied into `dir` so imports have somewhere to write.
fn write_library(dir: &Path) -> PathBuf {
    let path = dir.join("tones.json");
    std::fs::write(&path, tone_library::TONES_JSON).unwrap();
    path
}

fn import_args(settings: &Path, library: &Path) -> ImportArgs {
    ImportArgs {
        options: Options {
            settings: Some(settings.to_path_buf()),
            file: Some(library.to_path_buf()),
        },
        preset: Some("My Lead".to_owned()),
        program: Some(31),
        ..ImportArgs::default()
    }
}

#[test]
fn importing_keeps_an_ir_path_and_normalize_levels_the_tone() {
    let dir = scratch("import");
    let mut saved = preset(
        "My Lead",
        "electric",
        0.3,
        vec![BlockSetting::of(BlockChoice::Amp)],
    );
    let mut cabinet = BlockSetting::of(BlockChoice::Cabinet);
    cabinet.file = Some(PathBuf::from("/tmp/cab.wav"));
    saved.chain.blocks[2] = cabinet;
    let settings = write_settings(&dir, &[saved]);
    let library = write_library(&dir);

    // The import is a faithful copy: the level and the IR path are the preset's.
    run_import(import_args(&settings, &library)).unwrap();
    let written = load_library(&library).unwrap();
    let entry = written
        .tones
        .iter()
        .find(|tone| tone.name == "My Lead")
        .unwrap();
    assert_eq!(written.programs.get(&31), Some(&"My Lead".to_owned()));
    assert!((entry.sound.level - 0.3).abs() < 1e-6);
    assert!(entry.chain.blocks.iter().any(|block| block.file.is_some()));

    // `normalize` is the separate pass that levels it with the others.
    run_normalize(NormalizeArgs {
        options: Options {
            settings: Some(settings.clone()),
            file: Some(library.clone()),
        },
        tone: Some("My Lead".to_owned()),
        ..NormalizeArgs::default()
    })
    .unwrap();
    let written = load_library(&library).unwrap();
    let entry = written
        .tones
        .iter()
        .find(|tone| tone.name == "My Lead")
        .unwrap();
    let expected = crate::settings::StringSounds::default()
        .get(StringKind::Electric)
        .level;
    assert!((entry.sound.level - expected).abs() < 1e-6);
    // The measured pass sets the output knob (the chain's blocks are its sound, not its
    // level) and leaves that; the IR path survives too.
    let (low, high) = gscore_guitar::knob_ranges::OUTPUT;
    assert!(
        entry.chain.output_db >= low && entry.chain.output_db <= high,
        "{}",
        entry.chain.output_db
    );
    assert!(entry.chain.blocks.iter().any(|block| block.file.is_some()));

    // `--static` cancels the level trims at the output knob instead, without rendering.
    run_normalize(NormalizeArgs {
        options: Options {
            settings: Some(settings.clone()),
            file: Some(library.clone()),
        },
        tone: Some("My Lead".to_owned()),
        static_levels: true,
        ..NormalizeArgs::default()
    })
    .unwrap();
    let entry = load_library(&library)
        .unwrap()
        .tones
        .into_iter()
        .find(|tone| tone.name == "My Lead")
        .unwrap();
    assert!((entry.chain.output_db - (-chain_trim(&entry.chain))).abs() < 1e-4);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ir_drop_takes_the_file_out_and_keeps_the_cab_block() {
    let dir = scratch("drop");
    let mut saved = preset(
        "My Lead",
        "electric",
        0.3,
        vec![BlockSetting::of(BlockChoice::Amp)],
    );
    let mut cabinet = BlockSetting::of(BlockChoice::Cabinet);
    cabinet.file = Some(PathBuf::from("/tmp/cab.wav"));
    saved.chain.blocks[2] = cabinet;
    let settings = write_settings(&dir, &[saved]);
    let library = write_library(&dir);
    run_import(ImportArgs {
        drop_irs: true,
        ..import_args(&settings, &library)
    })
    .unwrap();
    let written = load_library(&library).unwrap();
    let entry = written
        .tones
        .iter()
        .find(|tone| tone.name == "My Lead")
        .unwrap();
    assert!(entry.chain.blocks.iter().all(|block| block.file.is_none()));
    assert_eq!(entry.chain.blocks[2].choice, BlockChoice::Cabinet);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_dry_run_writes_nothing() {
    let dir = scratch("dry-run");
    let saved = preset(
        "My Lead",
        "electric",
        0.3,
        vec![BlockSetting::of(BlockChoice::Amp)],
    );
    let settings = write_settings(&dir, &[saved]);
    let library = write_library(&dir);
    run_import(ImportArgs {
        dry_run: true,
        ..import_args(&settings, &library)
    })
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&library).unwrap(),
        tone_library::TONES_JSON
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dropping_a_cabinet_file_leaves_the_built_in_cabinet_and_a_room_goes() {
    let amp = BlockSetting::of(BlockChoice::Amp);
    let mut room = BlockSetting::of(BlockChoice::Room);
    room.file = Some(PathBuf::from("/tmp/room.wav"));
    let mut cabinet = BlockSetting::of(BlockChoice::Cabinet);
    cabinet.file = Some(PathBuf::from("/tmp/cab.wav"));
    let mut tone = Tone {
        kind: StringKind::Electric,
        sound: crate::settings::StringSound::default(),
        chain: ChainSetting {
            blocks: {
                let mut array = std::array::from_fn(|_| BlockSetting::default());
                array[0] = amp;
                array[1] = cabinet;
                array[2] = room;
                array
            },
            output_db: 0.0,
        },
    };
    let dropped = drop_impulse_responses(&mut tone);
    assert_eq!(dropped.len(), 2);
    assert_eq!(tone.chain.blocks[1].choice, BlockChoice::Cabinet);
    assert!(tone.chain.blocks[1].file.is_none());
    assert!(tone.chain.blocks[1].built_in_cabinet().is_some());
    assert_eq!(tone.chain.blocks[2].choice, BlockChoice::Empty);
}

#[test]
fn the_embedded_library_passes_the_checks() {
    validate(&library()).unwrap();
}

#[test]
fn a_program_naming_a_missing_tone_is_refused() {
    let mut file = library();
    file.programs.insert(30, "Not A Tone".to_owned());
    assert!(validate(&file).is_err());
}

#[test]
fn a_duplicate_name_is_refused() {
    let mut file = library();
    if let Some(entry) = file.tones.first().cloned() {
        file.tones.push(entry);
    }
    assert!(validate(&file).is_err());
}

#[test]
fn a_saved_tone_becomes_a_built_in_one() {
    let saved = preset(
        "My Lead",
        "electric",
        0.3,
        vec![BlockSetting::of(BlockChoice::Amp)],
    );
    let kind = kind_from_id(&saved.kind).unwrap();
    let mut tone = Tone {
        kind,
        sound: saved.sound.for_kind(kind),
        chain: saved.chain.clone(),
    };
    tone_library::normalize_static(&mut tone, 0.55);
    assert!((tone.sound.level - 0.55).abs() < 1e-6);
    assert!(chain_trim(&tone.chain).abs() < 1e-4);
}

#[test]
fn programs_list_their_names_kinds_and_tones() {
    let library = library();
    let lines = program_lines(&library, None);
    assert_eq!(lines.len(), 16, "programs 24 to 39");
    let nylon = &lines[0];
    assert!(
        nylon.contains(" 24 ") && nylon.contains("Nylon Str Guitar"),
        "{nylon}"
    );
    assert!(nylon.contains("nylon"), "{nylon}");
    assert!(
        nylon.contains(library.programs.get(&24).map_or("(none)", String::as_str)),
        "{nylon}"
    );
    assert!(lines[15].contains(" 39 ") && lines[15].contains("bass"));
    // A program without a tone says so, and an empty library is a valid one.
    let empty = ToneLibraryFile::default();
    assert!(validate(&empty).is_ok());
    let lines = program_lines(&empty, Some(&Settings::default()));
    assert!(
        lines.iter().all(|line| line.trim_end().ends_with("(none)")),
        "{lines:?}"
    );
}

#[test]
fn an_import_is_remembered_and_update_copies_a_changed_saved_tone_again() {
    let dir = scratch("update");
    let saved = preset(
        "My Lead",
        "electric",
        0.3,
        vec![BlockSetting::of(BlockChoice::Amp)],
    );
    let other = preset("My Clean", "electric", 0.5, Vec::new());
    let settings = write_settings(&dir, &[saved.clone(), other.clone()]);
    let library = write_library(&dir);
    run_import(ImportArgs {
        name: Some("Shipped Lead".to_owned()),
        description: Some("A lead.".to_owned()),
        ..import_args(&settings, &library)
    })
    .unwrap();
    run_import(ImportArgs {
        preset: Some("My Clean".to_owned()),
        program: Some(27),
        ..import_args(&settings, &library)
    })
    .unwrap();
    let lines = |settings: &Path| {
        program_lines(
            &load_library(&library).unwrap(),
            Some(&load_settings(settings).unwrap()),
        )
    };
    // Freshly imported: the program names the saved tone it came from, and nothing more.
    let listed = lines(&settings);
    assert!(listed[7].ends_with("[saved: My Lead]"), "{}", listed[7]);
    assert!(listed[3].ends_with("[saved: My Clean]"), "{}", listed[3]);

    // The saved lead is dialed in further and the clean one deleted: the list says so.
    let mut louder = saved;
    louder.sound.level = 0.45;
    let settings = write_settings(&dir, &[louder]);
    let listed = lines(&settings);
    assert!(
        listed[7].contains("[saved: My Lead, changed: run `tones update`]"),
        "{}",
        listed[7]
    );
    assert!(
        listed[3].contains("[saved: My Clean, no longer in the settings]"),
        "{}",
        listed[3]
    );

    // A dry run writes nothing; the update copies the lead again under its shipped name
    // and leaves the tone whose saved tone is gone.
    let update = |dry_run| {
        run_update(UpdateArgs {
            options: Options {
                settings: Some(settings.clone()),
                file: Some(library.clone()),
            },
            dry_run,
        })
    };
    update(true).unwrap();
    assert!(lines(&settings)[7].contains("changed"));
    update(false).unwrap();
    let written = load_library(&library).unwrap();
    let entry = |name: &str| written.tones.iter().find(|tone| tone.name == name).unwrap();
    assert!((entry("Shipped Lead").sound.level - 0.45).abs() < 1e-6);
    assert_eq!(entry("Shipped Lead").description, "A lead.");
    assert!((entry("My Clean").sound.level - other.sound.level).abs() < 1e-6);
    let listed = lines(&settings);
    assert!(listed[7].ends_with("[saved: My Lead]"), "{}", listed[7]);
    assert!(
        listed[3].contains("no longer in the settings"),
        "{}",
        listed[3]
    );
}

#[test]
fn an_rse_sound_is_mapped_to_a_tone_listed_and_forgotten_again() {
    let dir = scratch("rse");
    let settings = write_settings(
        &dir,
        &[preset(
            "My Lead",
            "electric",
            0.4,
            vec![BlockSetting::of(BlockChoice::Drive)],
        )],
    );
    let path = dir.join("tones.json");
    std::fs::write(&path, "{}").unwrap();
    // An import can name the Guitar Pro sounds that start on the tone too.
    run_import(ImportArgs {
        rse: vec!["Distortion Guitar/Lead".to_owned()],
        ..import_args(&settings, &path)
    })
    .unwrap();
    let rse_args = |map: &[&str], tone: Option<&str>, unmap: &[&str]| RseArgs {
        options: Options {
            settings: None,
            file: Some(path.clone()),
        },
        map: map.iter().map(|&sound| sound.to_owned()).collect(),
        tone: tone.map(str::to_owned),
        unmap: unmap.iter().map(|&sound| sound.to_owned()).collect(),
        ..RseArgs::default()
    };
    // And a sound can be mapped to a tone the library already has.
    run_rse(rse_args(
        &["Overdrive Guitar/Blues", "Clean Bass/Rock"],
        Some("My Lead"),
        &[],
    ))
    .unwrap();
    let file = load_library(&path).unwrap();
    assert_eq!(
        file.rse.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "Clean Bass/Rock",
            "Distortion Guitar/Lead",
            "Overdrive Guitar/Blues"
        ]
    );
    assert!(file.rse.values().all(|tone| tone == "My Lead"));

    // The sounds of a song: mapped ones name their tone, a tone of another kind of string
    // says it is not played, the rest have none.
    let mut found = rse::Found::default();
    for name in ["basic/staff.gp", "basic/steel_guitar_dadgad.gp"] {
        let song = gscore_session::open(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../testdata")
                .join(name),
        )
        .unwrap()
        .song;
        found.add(&song);
    }
    let lines = rse::found_lines(&file, &found);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Overdrive Guitar/Blues") && line.ends_with("My Lead")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("steel") && line.ends_with("(none)")),
        "{lines:?}"
    );
    let bass = rse::Found {
        sounds: BTreeMap::from([(("Clean Bass/Rock".to_owned(), Some("bass")), 2)]),
        ..rse::Found::default()
    };
    assert!(
        rse::found_lines(&file, &bass)
            .iter()
            .any(|line| line.contains("not played on bass strings")),
    );

    // A tone that is not in the library, or a sound that is not mapped, is refused.
    assert!(run_rse(rse_args(&["Jazz Guitar/Jazz ES"], Some("Gone"), &[])).is_err());
    assert!(run_rse(rse_args(&["Jazz Guitar/Jazz ES"], None, &[])).is_err());
    assert!(run_rse(rse_args(&[], None, &["Jazz Guitar/Jazz ES"])).is_err());
    run_rse(rse_args(&[], None, &["Clean Bass/Rock"])).unwrap();
    assert!(
        !load_library(&path)
            .unwrap()
            .rse
            .contains_key("Clean Bass/Rock")
    );
    // A library whose mapping names a tone that is gone does not validate.
    let mut broken = load_library(&path).unwrap();
    broken
        .rse
        .insert("Slap Bass/Chorus".to_owned(), "Gone".to_owned());
    assert!(validate(&broken).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

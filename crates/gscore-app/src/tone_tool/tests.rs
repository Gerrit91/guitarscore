//! Tests for the tone import tool.

use crate::chain_settings::{BlockSetting, Tone};
use crate::settings::kind_from_id;

use super::import::{drop_impulse_responses, run_import};
use super::normalize::run_normalize;
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
fn ir_drop_replaces_the_file_and_turns_the_speaker_on() {
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
    assert!((entry.chain.blocks[0].speaker - 1.0).abs() < 1e-6);
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
fn dropping_a_cabinet_turns_the_amps_speaker_on() {
    let mut amp = BlockSetting::of(BlockChoice::Amp);
    amp.speaker = 0.0;
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
                array
            },
            output_db: 0.0,
        },
    };
    let dropped = drop_impulse_responses(&mut tone);
    assert_eq!(dropped.len(), 1);
    assert!((tone.chain.blocks[0].speaker - 1.0).abs() < 1e-6);
    assert_eq!(tone.chain.blocks[1].choice, BlockChoice::Empty);
    assert!(tone.chain.blocks[1].file.is_none());
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
        nylon.contains(library.programs.get(&24).unwrap().as_str()),
        "{nylon}"
    );
    assert!(lines[15].contains(" 39 ") && lines[15].contains("bass"));
    // With settings: a program's own tone, and a saved tone standing in for another.
    let mut settings = Settings::default();
    settings.string_presets = vec![preset("Classical Guitar 1", "nylon", 0.4, Vec::new())];
    settings.preset_tones = vec![crate::settings::PresetTone {
        program: 30,
        kind: "electric".to_owned(),
        sound: crate::settings::StringSound::default(),
        chain: ChainSetting::default(),
    }];
    let lines = program_lines(&library, Some(&settings));
    assert!(
        lines[0].contains("[saved: Classical Guitar 1]"),
        "{}",
        lines[0]
    );
    assert!(
        lines[6].contains("[own tone on this machine]"),
        "{}",
        lines[6]
    );
}

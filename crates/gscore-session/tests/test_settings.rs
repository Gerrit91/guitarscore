//! The JSON settings file: a round trip, a missing file, broken and oversized files.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;

use gscore_session::{load_json, save_json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct TestSettings {
    theme: String,
    zoom: f32,
    recent: Vec<PathBuf>,
}

fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!(
        "gscore-settings-{name}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn settings_round_trip_through_the_file() {
    let dir = temp_dir("roundtrip");
    let path = dir.join("nested").join("settings.json");
    let settings = TestSettings {
        theme: "dark".to_owned(),
        zoom: 1.25,
        recent: vec![PathBuf::from("/tabs/song.gp")],
    };
    save_json(&path, &settings).unwrap();
    let loaded = load_json::<TestSettings>(&path);
    assert_eq!(loaded.value, settings);
    assert_eq!(loaded.path.as_deref(), Some(path.as_path()));
    assert_eq!(loaded.problem, None);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_missing_file_is_the_defaults_without_a_problem() {
    let dir = temp_dir("missing");
    let loaded = load_json::<TestSettings>(&dir.join("settings.json"));
    assert_eq!(loaded.value, TestSettings::default());
    assert_eq!(loaded.path, None);
    assert_eq!(loaded.problem, None);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn broken_and_oversized_files_fall_back_with_a_problem() {
    let dir = temp_dir("broken");
    let path = dir.join("settings.json");
    std::fs::write(&path, b"{ not json").unwrap();
    let loaded = load_json::<TestSettings>(&path);
    assert_eq!(loaded.value, TestSettings::default());
    assert!(
        loaded
            .problem
            .as_deref()
            .is_some_and(|p| p.contains("not a valid settings file")),
        "{:?}",
        loaded.problem
    );

    std::fs::write(&path, vec![b' '; 1024 * 1024 + 1]).unwrap();
    let loaded = load_json::<TestSettings>(&path);
    assert_eq!(loaded.value, TestSettings::default());
    assert!(
        loaded
            .problem
            .as_deref()
            .is_some_and(|p| p.contains("too large")),
        "{:?}",
        loaded.problem
    );
    std::fs::remove_dir_all(dir).unwrap();
}

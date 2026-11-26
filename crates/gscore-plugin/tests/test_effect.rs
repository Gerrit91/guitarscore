//! Hosting the test plugin (`examples/test_plugin.rs`, built by `cargo test`) as the app
//! hosts a user's plugin: opened from its library file, made an effect, run.

#![allow(clippy::unwrap_used)] // Helpers of the tests: a failing unwrap fails the test.
#![allow(unused_crate_dependencies)]

mod common;

use common::{GAIN, ID, library_path};
use gscore_plugin::{AudioConfig, Effect, Library, PluginError};

const CONFIG: AudioConfig = AudioConfig {
    sample_rate: 48_000,
    max_frames: 256,
};

fn library() -> Library {
    let path = library_path();
    assert!(
        path.exists(),
        "{} missing: run the tests with `cargo test` (it builds the examples)",
        path.display()
    );
    Library::open(&path).unwrap()
}

#[test]
fn the_library_lists_its_plugin() {
    let plugins = library().plugins();
    assert_eq!(plugins.len(), 1);
    let info = &plugins[0];
    assert_eq!(info.id, ID);
    assert_eq!(info.name, "GuitarScore Test Gain");
    assert_eq!(info.vendor, "GuitarScore");
    assert!(info.is_effect());
    assert!(!info.is_instrument());
}

#[test]
fn sound_runs_through_the_effect_and_follows_its_parameter() {
    let library = library();
    let mut effect = Effect::new(&library, ID).unwrap();
    let params = effect.params();
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "Gain");
    assert_eq!(
        (params[0].min, params[0].max, params[0].default),
        (0.0, 2.0, 1.0)
    );
    assert_eq!(effect.value_text(GAIN, 0.5).as_deref(), Some("0.50x"));
    assert_eq!(effect.latency(), 64);

    let mut processor = effect.activate(CONFIG).unwrap();
    // Longer than one block: split into blocks of `max_frames`.
    let mut left = vec![0.5_f32; 1000];
    let mut right = vec![-0.25_f32; 1000];
    processor.process(&mut left, &mut right).unwrap();
    assert!(left.iter().all(|&s| s == 0.5));
    assert!(right.iter().all(|&s| s == -0.25));

    processor.set_param(GAIN, 2.0);
    processor.process(&mut left, &mut right).unwrap();
    assert!(left.iter().all(|&s| s == 1.0));
    assert!(right.iter().all(|&s| s == -0.5));
    // Not a number: ignored.
    processor.set_param(GAIN, f64::NAN);
    processor.process(&mut left, &mut right).unwrap();
    assert!(left.iter().all(|&s| s == 2.0));
    assert!(!processor.failed());

    effect.deactivate(processor);
    assert_eq!(effect.value(GAIN), Some(2.0));
}

#[test]
fn settings_are_saved_and_given_back() {
    let library = library();
    let mut effect = Effect::new(&library, ID).unwrap();
    let mut processor = effect.activate(CONFIG).unwrap();
    processor.set_param(GAIN, 0.25);
    processor.process(&mut [0.0; 8], &mut [0.0; 8]).unwrap();
    effect.deactivate(processor);
    let blob = effect.save_state().unwrap();

    let mut other = Effect::new(&library, ID).unwrap();
    assert_eq!(other.value(GAIN), Some(1.0));
    other.load_state(&blob).unwrap();
    assert_eq!(other.value(GAIN), Some(0.25));
    // Broken settings: an error, the plugin keeps going.
    assert!(matches!(
        other.load_state(&[1, 2]),
        Err(PluginError::State { .. })
    ));
}

#[test]
fn the_effect_can_be_restarted_for_another_rate() {
    let library = library();
    let mut effect = Effect::new(&library, ID).unwrap();
    for rate in [44_100, 48_000, 96_000] {
        let mut processor = effect
            .activate(AudioConfig {
                sample_rate: rate,
                max_frames: 64,
            })
            .unwrap();
        processor.process(&mut [1.0; 100], &mut [1.0; 100]).unwrap();
        effect.deactivate(processor);
    }
    assert!(effect.idle().log.is_empty());
}

#[test]
fn a_missing_plugin_or_library_is_an_error() {
    let library = library();
    assert!(matches!(
        Effect::new(&library, "org.example.none"),
        Err(PluginError::NotFound(_))
    ));
    assert!(matches!(
        Effect::new(&library, "nul\0inside"),
        Err(PluginError::NotFound(_))
    ));
    let missing = library_path().with_file_name("no-such-plugin.clap");
    assert!(matches!(
        Library::open(&missing),
        Err(PluginError::Open { .. })
    ));
    // A file that is not a library at all.
    let junk = std::env::temp_dir().join(format!("gscore-junk-{}.clap", std::process::id()));
    std::fs::write(&junk, b"not a library").unwrap();
    assert!(matches!(
        Library::open(&junk),
        Err(PluginError::Open { .. })
    ));
    std::fs::remove_file(&junk).unwrap();
}

/// Dragonfly Room Reverb or any other CLAP effect named by `GSCORE_TEST_CLAP`
/// ("path/to/file.clap" or "path#plugin.id"): checked by hand, skipped otherwise.
#[test]
fn a_real_plugin_named_in_the_environment_runs() {
    let Ok(spec) = std::env::var("GSCORE_TEST_CLAP") else {
        return;
    };
    let (path, id) = spec
        .split_once('#')
        .map_or((spec.as_str(), None), |(p, i)| (p, Some(i)));
    let library = Library::open(path.as_ref()).unwrap();
    let plugins = library.plugins();
    let id = id.map_or_else(
        || plugins.iter().find(|p| p.is_effect()).unwrap().id.clone(),
        str::to_owned,
    );
    let mut effect = Effect::new(&library, &id).unwrap();
    assert!(!effect.params().is_empty());
    let mut processor = effect.activate(CONFIG).unwrap();
    let mut left = vec![0.0_f32; 48_000];
    let mut right = vec![0.0_f32; 48_000];
    left[0] = 1.0;
    right[0] = 1.0;
    processor.process(&mut left, &mut right).unwrap();
    assert!(left.iter().chain(&right).all(|s| s.is_finite()));
    effect.deactivate(processor);
    let blob = effect.save_state().unwrap();
    effect.load_state(&blob).unwrap();
}

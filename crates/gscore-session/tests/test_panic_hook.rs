//! The panic hook writes a crash report file. Runs in its own test binary because the hook is
//! process-global.

use std::path::PathBuf;

fn unique_temp_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!(
        "gscore-panic-hook-test-{}-{nanos}",
        std::process::id()
    ))
}

#[test]
fn panic_writes_a_crash_report() {
    let dir = unique_temp_dir();
    gscore_session::install_panic_hook(dir.clone());

    let result = std::thread::Builder::new()
        .name("crashing-worker".to_owned())
        .spawn(|| panic!("deliberate test panic"))
        .unwrap()
        .join();
    assert!(result.is_err(), "the thread should have panicked");

    let reports: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(reports.len(), 1, "expected exactly one report: {reports:?}");
    let report = std::fs::read_to_string(&reports[0]).unwrap();
    assert!(reports[0].extension().is_some_and(|e| e == "log"));
    assert!(
        report.contains("message: deliberate test panic"),
        "{report}"
    );
    assert!(report.contains("thread: crashing-worker"), "{report}");
    assert!(report.contains("test_panic_hook.rs"), "{report}");
    assert!(report.contains("backtrace:"), "{report}");

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn default_crash_dir_is_inside_an_app_directory() {
    if let Some(dir) = gscore_session::default_crash_dir() {
        assert!(dir.to_string_lossy().contains("guitarscore"), "{dir:?}");
    }
}

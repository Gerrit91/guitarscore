//! `gui`: starts the desktop application.
//!
//! The application is a separate binary (`guitarscore`, in `gscore-app`, the same layer as
//! this crate), so it is started as a child process rather than linked in: that keeps the
//! layering table intact and lets a user who installed both run either one.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::CliError;

/// Starts the desktop application, opening `file` if one was given, and waits for it to
/// exit. The application's own messages go to this process's stdout and stderr.
pub(crate) fn run(file: Option<&Path>) -> Result<String, CliError> {
    // A mistyped file name is reported here, before a window opens.
    if let Some(file) = file
        && !file.exists()
    {
        return Err(CliError::Failed(format!(
            "{} does not exist",
            file.display()
        )));
    }
    let binary = find_app()?;
    launch(&binary, file)?;
    Ok(String::new())
}

/// The application's file name, with this platform's `.exe` suffix on Windows.
fn app_binary_name() -> String {
    format!("guitarscore{}", std::env::consts::EXE_SUFFIX)
}

/// Finds the application: first next to this executable (a workspace build has both in
/// `target/<profile>/`), then in the directories on `PATH`.
fn find_app() -> Result<PathBuf, CliError> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        dirs.push(dir);
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    find_app_in(dirs).ok_or_else(|| {
        CliError::Failed(format!(
            "cannot find the GuitarScore application (`{}`) next to gscore-cli or on PATH; \
             build it with `cargo build -p gscore-app`, or install it, and try again",
            app_binary_name()
        ))
    })
}

/// The first directory in `dirs` that contains the application's binary.
fn find_app_in(dirs: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    let name = app_binary_name();
    dirs.into_iter()
        .map(|dir| dir.join(&name))
        .find(|c| c.is_file())
}

/// Runs the application at `binary` with `file` as its only argument and waits for it.
fn launch(binary: &Path, file: Option<&Path>) -> Result<(), CliError> {
    let mut command = Command::new(binary);
    if let Some(file) = file {
        command.arg(file);
    }
    let status = command
        .status()
        .map_err(|e| CliError::Failed(format!("cannot start `{}`: {e}", binary.display())))?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::Failed(format!(
            "`{}` failed ({status})",
            binary.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir =
            std::env::temp_dir().join(format!("gscore-gui-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_application_is_found_in_the_first_directory_that_has_it() {
        let first = temp_dir("empty");
        let second = temp_dir("has-app");
        let binary = second.join(app_binary_name());
        std::fs::write(&binary, b"not really a program").unwrap();

        let found = find_app_in([first.clone(), second, first.clone()]);
        assert_eq!(found, Some(binary.clone()), "the sooner directory wins");
        assert!(binary.is_file());
        for dir in [first, binary.parent().unwrap().to_path_buf()] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn a_directory_named_like_the_application_is_not_the_application() {
        let dir = temp_dir("dir-only");
        std::fs::create_dir(dir.join(app_binary_name())).unwrap();
        assert_eq!(find_app_in([dir.clone()]), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn no_application_means_no_result() {
        assert_eq!(find_app_in([]), None);
    }

    #[cfg(unix)]
    #[test]
    fn launch_waits_and_passes_the_file_through() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("launch");
        let args_file = dir.join("args.txt");
        let script = dir.join("guitarscore");
        let body = format!(
            "#!/bin/sh\nprintf '%s' \"$*\" > '{}'\n",
            args_file.display()
        );
        std::fs::write(&script, body).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let song = dir.join("song.gp");
        std::fs::write(&song, b"score").unwrap();

        launch(&script, Some(&song)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&args_file).unwrap(),
            song.display().to_string(),
            "the file is the application's argument"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn launch_reports_an_application_that_fails() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("fails");
        let script = dir.join("guitarscore");
        std::fs::write(&script, "#!/bin/sh\nexit 3\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let err = launch(&script, None).unwrap_err();
        assert!(err.to_string().contains('3'), "{err}");
        let _ = std::fs::remove_dir_all(dir);
    }
}

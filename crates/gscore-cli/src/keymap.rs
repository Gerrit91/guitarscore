//! `keymap`: the app's key bindings. Prints the built-in keymap (a valid keymap file to
//! start from) with the action ids, or checks a keymap file and lists its problems.

use std::path::Path;

use gscore_edit::{ACTIONS, DEFAULT_KEYMAP, Keymap};

use crate::CliError;

/// Prints where the app reads its keymap, the action ids, and the built-in keymap; or, with
/// `check`, validates that file.
pub(crate) fn run(check: Option<&Path>) -> Result<String, CliError> {
    if let Some(path) = check {
        let loaded = gscore_session::load_keymap(path);
        return match (loaded.path, loaded.problem) {
            (None, _) => Err(CliError::Failed(format!(
                "{} does not exist",
                path.display()
            ))),
            (Some(_), Some(problem)) => Err(CliError::Failed(problem)),
            (Some(path), None) => Ok(format!("{} is a valid keymap.\n", path.display())),
        };
    }
    let mut out = String::new();
    match gscore_session::default_config_dir() {
        Some(dir) => out.push_str(&format!(
            "The app reads its keymap from {}\n(missing: the built-in keymap below is used).\n\n",
            dir.join(gscore_session::KEYMAP_FILE).display()
        )),
        None => out.push_str("No config directory is known (HOME is not set).\n\n"),
    }
    out.push_str("Actions:\n");
    let keymap = Keymap::default();
    for action in ACTIONS {
        let keys: Vec<String> = keymap
            .keys_for(action.action)
            .iter()
            .map(ToString::to_string)
            .collect();
        let keys = if keys.is_empty() {
            "(unbound)".to_owned()
        } else {
            keys.join(", ")
        };
        out.push_str(&format!(
            "  {:<20} {:<18} {keys}\n",
            action.id, action.label
        ));
    }
    out.push_str("\nBuilt-in keymap:\n\n");
    out.push_str(DEFAULT_KEYMAP);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use crate::run_cli;

    #[test]
    fn keymap_lists_actions_and_checks_files() {
        let out = run_cli(["keymap"]).unwrap();
        assert!(
            out.contains("cursor_left") && out.contains("ctrl+z"),
            "{out}"
        );
        assert!(out.contains("Right = cursor_right"), "{out}");
        let dir = std::env::temp_dir().join(format!("gscore-cli-keymap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("keymap.txt");
        std::fs::write(
            &file,
            "F2 = undo
",
        )
        .unwrap();
        let path = file.to_str().unwrap();
        assert!(
            run_cli(["keymap", "--check", path])
                .unwrap()
                .contains("valid")
        );
        std::fs::write(
            &file,
            "F2 = nope
",
        )
        .unwrap();
        let error = run_cli(["keymap", "--check", path])
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("line 1") && error.contains("nope"),
            "{error}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

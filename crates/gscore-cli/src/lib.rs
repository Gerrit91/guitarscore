//! Headless command-line tools for working with Guitar Pro files without the GUI.
//!
//! Every subcommand is listed in [`COMMANDS`], which drives both dispatch and `--help`.
//! A new subcommand gets a module, a row in that table and a match arm in [`run_cli`].

mod help;
mod info;

use std::fmt;

/// One subcommand: its name, argument synopsis, one-line summary and longer description.
#[derive(Debug)]
struct CommandSpec {
    name: &'static str,
    args: &'static str,
    summary: &'static str,
    details: &'static str,
}

/// All subcommands, in the order `--help` lists them.
const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "info",
        args: "<file>",
        summary: "Show format, title, tempo, tracks and tunings of a Guitar Pro file",
        details: "Reads a Guitar Pro file and prints its format, metadata, measure count, and \
                  each track with its tuning (MIDI pitches, string 1 first). Then lists issues \
                  that do not prevent opening the file, such as measures that are not \
                  filled or several notes on one string (the first 10, and how many more).\n\
                  The format is detected from the file's content, not its extension.",
    },
    CommandSpec {
        name: "help",
        args: "[command]",
        summary: "Show this help, or the help for one command",
        details: "Without an argument, lists all commands. With a command name, shows that \
                  command's usage and description.",
    },
];

/// Why the CLI did not succeed. Decides the exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// The command line was wrong (unknown command, missing argument). Exit code 2.
    Usage(String),
    /// The command ran but failed (unreadable or invalid file). Exit code 1.
    Failed(String),
}

impl CliError {
    /// The process exit code for this error.
    pub fn exit_code(&self) -> u8 {
        match self {
            CliError::Usage(_) => 2,
            CliError::Failed(_) => 1,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Usage(msg) | CliError::Failed(msg) => f.write_str(msg),
        }
    }
}

/// Installs the panic hook that writes crash reports to the per-user crash directory.
/// Call once at the start of `main`.
pub fn install_crash_reporting() {
    if let Some(dir) = gscore_session::default_crash_dir() {
        gscore_session::install_panic_hook(dir);
    }
}

/// Runs the CLI with the arguments after the program name. Returns the text to print on
/// stdout on success.
pub fn run_cli(args: &[String]) -> Result<String, CliError> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] => Err(CliError::Usage(help::overview())),
        ["-h" | "--help"] | ["help"] => Ok(help::overview()),
        ["-V" | "--version"] => Ok(format!("gscore-cli {}\n", env!("CARGO_PKG_VERSION"))),
        ["help", command] | [command, "-h" | "--help"] => help::command(command),
        ["info", path] => info::run(path),
        [command, ..] => match find_command(command) {
            Some(spec) => Err(CliError::Usage(format!(
                "wrong arguments for `{}`\n\n{}",
                spec.name,
                help::usage_of(spec)
            ))),
            None => Err(CliError::Usage(format!(
                "unknown command `{command}`\n\n{}",
                help::overview()
            ))),
        },
    }
}

fn find_command(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|spec| spec.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<String, CliError> {
        run_cli(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn help_lists_every_command() {
        for args in [&["--help"][..], &["-h"], &["help"]] {
            let out = run(args).unwrap();
            for spec in COMMANDS {
                assert!(out.contains(spec.name), "{args:?}: {out}");
                assert!(out.contains(spec.summary), "{args:?}: {out}");
            }
        }
    }

    #[test]
    fn command_help_shows_usage_and_details() {
        for args in [&["help", "info"][..], &["info", "--help"], &["info", "-h"]] {
            let out = run(args).unwrap();
            assert!(out.contains("Usage: gscore-cli info <file>"), "{out}");
            assert!(out.contains("detected from the file's content"), "{out}");
        }
        assert!(matches!(run(&["help", "bogus"]), Err(CliError::Usage(_))));
    }

    #[test]
    fn version_is_printed() {
        assert!(run(&["--version"]).unwrap().starts_with("gscore-cli "));
    }

    #[test]
    fn bad_arguments_are_usage_errors_with_exit_code_2() {
        for args in [
            &[][..],
            &["info"],
            &["info", "a", "b"],
            &["bogus"],
            &["--bogus"],
        ] {
            let err = run(args).unwrap_err();
            assert!(matches!(err, CliError::Usage(_)), "{args:?}");
            assert_eq!(err.exit_code(), 2);
        }
    }

    #[test]
    fn unreadable_file_is_a_failure_with_exit_code_1() {
        let err = run(&["info", "/nonexistent/file.gp"]).unwrap_err();
        assert!(matches!(err, CliError::Failed(_)), "{err:?}");
        assert_eq!(err.exit_code(), 1);
    }
}

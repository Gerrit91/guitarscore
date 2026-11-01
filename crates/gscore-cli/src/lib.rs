//! Headless command-line tools for working with Guitar Pro files without the GUI.
//!
//! Argument parsing and help come from clap: every subcommand is a variant of [`Command`],
//! and its doc comment is its help text. A new subcommand gets a variant, a module, and a
//! match arm in [`run_cli`].

mod dump;
mod info;
mod input;
mod legend;
mod roundtrip;

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{Parser, Subcommand};

/// Headless tools for Guitar Pro files.
#[derive(Debug, Parser)]
#[command(
    name = "gscore-cli",
    version,
    after_help = "Supported formats: .gp3, .gp4, .gp5, .gpx (Guitar Pro 6), .gp (Guitar Pro 7+)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show format, title, tempo, tracks and tunings of a Guitar Pro file
    ///
    /// Prints the file's format, metadata, measure count, and each track with its tuning
    /// (note names, lowest string first). Then lists issues that do not prevent opening the
    /// file, such as measures that are not filled or several notes on one string (the first
    /// 10, and how many more). The format is detected from the file's content, not its
    /// extension.
    Info {
        /// The Guitar Pro file to read
        file: PathBuf,
    },
    /// Print the tablature of a Guitar Pro file as ASCII tab
    ///
    /// One row per string, highest first, labelled with the open string's note name. Each
    /// beat is as wide as its length (a whole note is 24 columns), all voices share one time
    /// grid, and long tracks wrap at measure boundaries. Dead notes are shown as `x`, tied
    /// notes as `L`. Effects use the usual ASCII-tab symbols: `h`/`p`, `b`/`r` with the
    /// target fret, `/`/`\`, `~`, `tr`, `t`, `TP`, `*` and harmonic nodes in brackets,
    /// `>` and `.`. Palm mutes and let ring go on rows under the staff (`PM----|`,
    /// `LR----|`). `gscore-cli legend` lists every symbol. Tracks that are not string
    /// instruments (drums, voice, ...) are listed without tablature.
    Dump {
        /// The Guitar Pro file to read
        file: PathBuf,
        /// Only this track (1-based, as `info` numbers them)
        #[arg(long)]
        track: Option<usize>,
        /// Maximum line length before wrapping to the next system
        #[arg(long, default_value_t = 80)]
        width: usize,
    },
    /// List the symbols `dump` writes in its ASCII tab
    ///
    /// Prints what every marker after a fret or beat means, and what the rows under the
    /// staff (`PM`, `LR`) mark.
    Legend,
    /// Write a file, read it back and report what changed (development tool)
    ///
    /// Writes the song in its own format (or `--to`), reads the result back and compares
    /// song information, tracks, tunings, time signatures, beats and notes. Exit code 0 if
    /// nothing changed, 1 with a list of differences otherwise. Hidden from `--help`: it
    /// checks GuitarScore itself and is not meant for normal use.
    #[command(hide = true)]
    RoundtripCheck {
        /// The Guitar Pro file to read
        file: PathBuf,
        /// Write in this format instead of the file's own
        #[arg(long, value_enum)]
        to: Option<roundtrip::FormatArg>,
    },
}

/// Why the CLI did not succeed. Decides the exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// The command line was wrong (unknown command, missing argument). Exit code 2.
    /// The message is complete, including usage, and printed as is.
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
            CliError::Usage(msg) => f.write_str(msg),
            CliError::Failed(msg) => write!(f, "Error: {msg}"),
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
/// stdout on success (including `--help` and `--version` output).
pub fn run_cli<I, T>(args: I) -> Result<String, CliError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let argv =
        std::iter::once(OsString::from("gscore-cli")).chain(args.into_iter().map(Into::into));
    let cli = match Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(err) => {
            let text = err.render().to_string();
            return match err.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => Ok(text),
                _ => Err(CliError::Usage(text)),
            };
        }
    };
    match cli.command {
        Command::Info { file } => info::run(&file),
        Command::Dump { file, track, width } => dump::run(&file, track, width),
        Command::Legend => Ok(legend::run()),
        Command::RoundtripCheck { file, to } => roundtrip::run(&file, to),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn command_definitions_are_consistent() {
        // Catches mistakes in the clap definitions (duplicate flags, bad defaults, ...).
        Cli::command().debug_assert();
    }

    #[test]
    fn help_lists_every_command() {
        for args in [&["--help"][..], &["-h"], &["help"]] {
            let out = run_cli(args).unwrap();
            assert!(out.contains("info"), "{args:?}: {out}");
            assert!(out.contains("Show format, title, tempo"), "{args:?}: {out}");
            assert!(out.contains("Supported formats"), "{args:?}: {out}");
        }
    }

    #[test]
    fn command_help_shows_usage_and_details() {
        for args in [&["help", "info"][..], &["info", "--help"]] {
            let out = run_cli(args).unwrap();
            assert!(out.contains("Usage: gscore-cli info <FILE>"), "{out}");
            assert!(out.contains("detected from the file's content"), "{out}");
        }
    }

    #[test]
    fn roundtrip_check_is_hidden_but_works() {
        let help = run_cli(["--help"]).unwrap();
        assert!(!help.contains("roundtrip"), "{help}");
        let own_help = run_cli(["roundtrip-check", "--help"]).unwrap();
        assert!(
            own_help.contains("Usage: gscore-cli roundtrip-check"),
            "{own_help}"
        );
    }

    #[test]
    fn version_is_printed() {
        assert!(run_cli(["--version"]).unwrap().starts_with("gscore-cli "));
    }

    #[test]
    fn bad_arguments_are_usage_errors_with_exit_code_2() {
        for args in [
            &[][..],
            &["info"],
            &["info", "a", "b"],
            &["dump"],
            &["dump", "a", "--width", "wide"],
            &["bogus"],
            &["--bogus"],
        ] {
            let err = run_cli(args).unwrap_err();
            assert!(matches!(err, CliError::Usage(_)), "{args:?}: {err:?}");
            assert_eq!(err.exit_code(), 2);
        }
    }

    #[test]
    fn typos_get_a_suggestion() {
        let err = run_cli(["inof", "x.gp"]).unwrap_err().to_string();
        assert!(err.contains("info"), "{err}");
    }

    #[test]
    fn unreadable_file_is_a_failure_with_exit_code_1() {
        let err = run_cli(["info", "/nonexistent/file.gp"]).unwrap_err();
        assert!(matches!(err, CliError::Failed(_)), "{err:?}");
        assert_eq!(err.exit_code(), 1);
    }
}

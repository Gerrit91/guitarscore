//! Headless command-line tools for working with Guitar Pro files without the GUI.
//!
//! Argument parsing and help come from clap: every subcommand is a variant of [`Command`],
//! and its doc comment is its help text. A new subcommand gets a variant, a module, and a
//! match arm in [`run_cli`].

mod bench;
mod commands;
mod convert;
mod dump;
mod edit;
mod format_arg;
mod gui;
mod info;
mod input;
mod keymap;
mod legend;
mod play;
mod render;
mod report;
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
    /// Write a song as a MIDI file (.mid)
    ///
    /// Plays the song through the sequencer and writes a Standard MIDI File: one track per
    /// song track with its instrument, volume and pan, tempo changes, and repeats played
    /// out. Ties are held, dead notes are short, harmonics sound at their harmonic pitch.
    /// Bends, slides and other effects are not played yet.
    Midi {
        /// The Guitar Pro file to read
        file: PathBuf,
        /// The MIDI file to write
        #[arg(short, long)]
        output: PathBuf,
        /// Only these tracks (1-based; repeat for several)
        #[arg(long = "track", value_name = "N")]
        tracks: Vec<usize>,
    },
    /// List the MIDI output ports `play` can use
    Ports,
    /// Play a song with the built-in synthesizer or to a MIDI output port
    ///
    /// Plays the song and returns when it has finished. By default the built-in
    /// synthesizer plays it on the default audio device, with the SoundFont (.sf2) given by
    /// --soundfont, the GSCORE_SOUNDFONT variable, or the first one installed in the usual
    /// places (/usr/share/soundfonts, /usr/share/sounds/sf2, ~/.local/share/soundfonts).
    /// With --port, or when no SoundFont is found, the song goes to a MIDI port instead,
    /// such as FluidSynth or a hardware synth; `gscore-cli ports` lists them.
    Play {
        /// The Guitar Pro file to read
        file: PathBuf,
        /// Play to this MIDI output port (part of its name, as `ports` lists it)
        #[arg(long, conflicts_with = "soundfont")]
        port: Option<String>,
        /// Play with the built-in synthesizer and this SoundFont (.sf2)
        #[arg(long, value_name = "FILE")]
        soundfont: Option<PathBuf>,
        /// Start at this measure (1-based; its first pass if it repeats)
        #[arg(long, value_name = "MEASURE", default_value_t = 1)]
        from: usize,
        /// Only these tracks (1-based; repeat for several)
        #[arg(long = "track", value_name = "N")]
        tracks: Vec<usize>,
        /// Speed factor: 0.5 is half speed, 2 double speed
        #[arg(long, default_value_t = 1.0)]
        speed: f32,
    },
    /// Draw one track's score as an SVG image or a PDF
    ///
    /// Lays out one track (title, tuning, tempo, then the measures broken into lines on an
    /// A4-wide page: standard notation and tab as the track shows them, a percussion staff
    /// for drums) and writes it as SVG, or as PDF when <OUTPUT> ends in .pdf. The SVG uses
    /// the Bravura and Noto Sans fonts by name (bundled in assets/fonts/); install them to
    /// view it correctly. The PDF lays the track out on pages with the song's page setup
    /// (paper, margins, score size, header and footer) and embeds the fonts.
    Render {
        /// The Guitar Pro file to read
        file: PathBuf,
        /// The SVG or PDF file to write
        #[arg(short, long)]
        output: PathBuf,
        /// The track to draw (1-based, as `info` numbers them)
        #[arg(long)]
        track: Option<usize>,
        /// Color theme
        #[arg(long, value_enum, default_value = "light")]
        theme: render::ThemeArg,
    },
    /// Save a Guitar Pro file in another (or the same) format
    ///
    /// Reads <FILE> and writes it to <OUTPUT> in the format given by --to or by the output's
    /// extension. Only Guitar Pro 7+ (.gp) can be written for now. The song is checked
    /// before anything is written: if the format would lose something, the differences are
    /// listed and nothing is written (unless --allow-losses). An existing output file is kept
    /// as `<OUTPUT>.bak` the first time it is overwritten, and the new file replaces it
    /// atomically.
    Convert {
        /// The Guitar Pro file to read
        file: PathBuf,
        /// The file to write
        #[arg(short, long)]
        output: PathBuf,
        /// Write in this format instead of the one the output's extension names
        #[arg(long, value_enum)]
        to: Option<format_arg::FormatArg>,
        /// Save even if the format cannot keep everything (the losses are listed)
        #[arg(long)]
        allow_losses: bool,
    },
    /// Apply editing commands to a Guitar Pro file
    ///
    /// Reads <FILE>, applies the commands, and saves the result with the safe-save procedure
    /// (only Guitar Pro 7+ files, .gp, can be written for now). Commands are given with -c
    /// (repeatable) and/or in a script file (--script). Positions are 1-based. Commands from
    /// -c run in the order given, then the script's lines. If a command fails, nothing is
    /// written.
    ///
    /// Examples:
    ///   gscore-cli edit song.gp -o out.gp -c "set_fret track=1 measure=3 beat=2 string=6 fret=5"
    ///   gscore-cli edit song.gp -o out.gp --script edits.txt
    ///
    /// `gscore-cli commands` lists every command and its parameters.
    Edit {
        /// The Guitar Pro file to read
        file: PathBuf,
        /// The file to write
        #[arg(short, long)]
        output: PathBuf,
        /// A command to apply; repeat for several commands
        #[arg(short = 'c', long = "command", value_name = "COMMAND")]
        commands: Vec<String>,
        /// A script file with one command per line (blank lines and # comments are ignored)
        #[arg(long, value_name = "FILE")]
        script: Option<PathBuf>,
        /// Save even if the format cannot keep everything (the losses are listed)
        #[arg(long)]
        allow_losses: bool,
    },
    /// List the editing commands `gscore-cli edit` accepts
    ///
    /// Prints every command id (and alias), what it does, and its parameters: whether they
    /// are required and their defaults. Positions are 1-based.
    Commands,
    /// Time the work behind a keystroke on real files
    ///
    /// Opens each file and reports medians of several runs: opening, laying out the
    /// busiest track, building the playback sequence, editing a note and inserting a
    /// measure (each with undo), and an edit plus the layout after it as the app does it.
    /// Build with --release for meaningful numbers (`cargo xtask bench` does).
    Bench {
        /// Guitar Pro files to time
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Fail if a budget is missed: edit and layout within 16 ms, opening within 2 s
        #[arg(long)]
        check: bool,
    },
    /// List the symbols `dump` writes in its ASCII tab
    ///
    /// Prints what every marker after a fret or beat means, and what the rows under the
    /// staff (`PM`, `LR`) mark.
    Legend,
    /// Show the app's key bindings, or check a keymap file
    ///
    /// Without options: where the app reads its keymap file, every action id with its
    /// keys, and the built-in keymap, which is a valid keymap file to start from. A binding
    /// names the typed character (`+`, `5`), not the key's position, so it works on any
    /// keyboard layout.
    Keymap {
        /// Check this keymap file and list its problems
        #[arg(long)]
        check: Option<PathBuf>,
    },
    /// Start the desktop application
    ///
    /// Runs the `guitarscore` application, opening <FILE> if one is given, and waits until
    /// it exits; this command fails if the application cannot be started or exits with an
    /// error. The application is looked for next to `gscore-cli` (a workspace build puts
    /// both in `target/<profile>/`), then on `PATH`; build it with
    /// `cargo build -p gscore-app` if it is missing.
    Gui {
        /// The Guitar Pro file to open
        file: Option<PathBuf>,
    },
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
        to: Option<format_arg::FormatArg>,
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
        Command::Render {
            file,
            output,
            track,
            theme,
        } => render::run(&file, &output, track, theme),
        Command::Convert {
            file,
            output,
            to,
            allow_losses,
        } => convert::run(&file, &output, to, allow_losses),
        Command::Edit {
            file,
            output,
            commands,
            script,
            allow_losses,
        } => edit::run(&file, &output, &commands, script.as_deref(), allow_losses),
        Command::Commands => Ok(commands::run()),
        Command::Midi {
            file,
            output,
            tracks,
        } => play::export(&file, &output, &tracks),
        Command::Ports => play::ports(),
        Command::Play {
            file,
            port,
            soundfont,
            from,
            tracks,
            speed,
        } => play::play(
            &file,
            &play::Options {
                port: port.as_deref(),
                soundfont,
                from,
                tracks: &tracks,
                speed,
            },
        ),
        Command::Bench { files, check } => bench::run(&files, check),
        Command::Legend => Ok(legend::run()),
        Command::Keymap { check } => keymap::run(check.as_deref()),
        Command::Gui { file } => gui::run(file.as_deref()),
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
            assert!(
                out.contains("Start the desktop application"),
                "{args:?}: {out}"
            );
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
    fn gui_help_names_the_application_and_the_optional_file() {
        let out = run_cli(["help", "gui"]).unwrap();
        assert!(out.contains("Usage: gscore-cli gui [FILE]"), "{out}");
        assert!(out.contains("guitarscore"), "{out}");
        assert!(out.contains("cargo build -p gscore-app"), "{out}");
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

    #[test]
    fn gui_refuses_a_missing_file_before_starting_anything() {
        // Only the failure path is tested: a successful call would open a window.
        let err = run_cli(["gui", "/nonexistent/file.gp"]).unwrap_err();
        assert!(
            matches!(err, CliError::Failed(ref m) if m.contains("does not exist")),
            "{err:?}"
        );
    }
}

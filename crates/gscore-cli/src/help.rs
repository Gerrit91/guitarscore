//! Help texts, generated from the command table.

use crate::{find_command, CliError, CommandSpec, COMMANDS};

/// The full help: usage, all commands, options.
pub(crate) fn overview() -> String {
    let width = COMMANDS
        .iter()
        .map(|c| c.name.len().saturating_add(c.args.len()).saturating_add(1))
        .max()
        .unwrap_or(0)
        .max("-V, --version".len());
    let mut out = format!(
        "gscore-cli {} - headless tools for Guitar Pro files\n\n\
         Usage: gscore-cli <command> [arguments]\n\n\
         Commands:\n",
        env!("CARGO_PKG_VERSION")
    );
    for spec in COMMANDS {
        let synopsis = format!("{} {}", spec.name, spec.args);
        out.push_str(&format!("  {synopsis:<width$}  {}\n", spec.summary));
    }
    out.push_str(&format!(
        "\nOptions:\n  {:<width$}  Show this help (or `<command> --help` for one command)\n  \
         {:<width$}  Show the version\n\n\
         Supported formats: .gp3, .gp4, .gp5, .gpx (Guitar Pro 6), .gp (Guitar Pro 7+)\n",
        "-h, --help", "-V, --version"
    ));
    out
}

/// Help for one command, or a usage error if there is no such command.
pub(crate) fn command(name: &str) -> Result<String, CliError> {
    match find_command(name) {
        Some(spec) => Ok(format!("{}\n{}\n", usage_of(spec), spec.details)),
        None => Err(CliError::Usage(format!(
            "unknown command `{name}`\n\n{}",
            overview()
        ))),
    }
}

/// "Usage: ..." line and summary of one command.
pub(crate) fn usage_of(spec: &CommandSpec) -> String {
    format!(
        "Usage: gscore-cli {} {}\n\n{}\n",
        spec.name, spec.args, spec.summary
    )
}

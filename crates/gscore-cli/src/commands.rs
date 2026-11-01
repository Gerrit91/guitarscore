//! `commands`: what `gscore-cli edit` accepts, with parameters and defaults.

/// Lists every editing command, its parameters and their defaults.
pub(crate) fn run() -> String {
    let mut out = String::from(
        "Commands (positions are 1-based).\n\
         Usage: gscore-cli edit <in> -o <out> -c \"<command> key=value ...\"\n\
         or:    gscore-cli edit <in> -o <out> --script <file>\n\n",
    );
    for info in gscore_edit::COMMANDS {
        out.push_str(info.id);
        if !info.aliases.is_empty() {
            out.push_str(&format!(" (alias: {})", info.aliases.join(", ")));
        }
        out.push_str(&format!(" - {}\n  {}\n", info.label, info.help));
        for param in info.params {
            let requirement = match (param.required, param.default) {
                (true, _) => "required".to_owned(),
                (false, Some(default)) => format!("default {default}"),
                (false, None) => "optional".to_owned(),
            };
            out.push_str(&format!(
                "  {:<10} {:<14} {}\n",
                param.name, requirement, param.help
            ));
        }
        out.push('\n');
    }
    out
}

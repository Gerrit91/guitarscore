//! `audit`: what saving GP7 files with GuitarScore leaves out of them (see
//! `gscore_io::audit`), over files and folders.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use gscore_io::audit::{Verdict, gp7_save_losses, verdict};

use crate::CliError;

/// The `.gp` files among `paths`, folders searched through, sorted.
fn gp_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(path: &Path, found: &mut Vec<PathBuf>) {
        if path.is_dir() {
            let Ok(entries) = std::fs::read_dir(path) else {
                return;
            };
            for entry in entries.flatten() {
                walk(&entry.path(), found);
            }
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gp"))
        {
            found.push(path.to_path_buf());
        }
    }
    let mut found = Vec::new();
    for path in paths {
        walk(path, &mut found);
    }
    found.sort();
    found
}

/// What one key lost over all files: how many occurrences, in how many files, and one of
/// them (to look at).
#[derive(Default)]
struct Total {
    lost: usize,
    files: usize,
    example: Option<PathBuf>,
}

/// Audits the GP7 files in `paths`. With `check`, an unexplained loss is a failure; with
/// `verbose`, the keys Guitar Pro rebuilds are listed too.
pub(crate) fn run(paths: &[PathBuf], check: bool, verbose: bool) -> Result<String, CliError> {
    let files = gp_files(paths);
    if files.is_empty() {
        return Err(CliError::Failed("no .gp files found".to_owned()));
    }
    let mut totals: BTreeMap<String, Total> = BTreeMap::new();
    let mut unreadable: Vec<String> = Vec::new();
    for file in &files {
        let losses = std::fs::read(file)
            .map_err(|error| error.to_string())
            .and_then(|data| gp7_save_losses(&data).map_err(|error| error.to_string()));
        match losses {
            Ok(losses) => {
                for loss in losses {
                    let total = totals.entry(loss.key).or_default();
                    total.lost = total
                        .lost
                        .saturating_add(loss.before.saturating_sub(loss.after));
                    total.files = total.files.saturating_add(1);
                    total.example.get_or_insert_with(|| file.clone());
                }
            }
            Err(error) => unreadable.push(format!("{}: {error}", file.display())),
        }
    }
    let mut new: Vec<(&String, &Total)> = Vec::new();
    let mut known: Vec<(&String, &Total, &str)> = Vec::new();
    let mut rebuilt: Vec<(&String, &Total, &str)> = Vec::new();
    for (key, total) in &totals {
        match verdict(key) {
            Verdict::Unexplained => new.push((key, total)),
            Verdict::Known(why) => known.push((key, total, why)),
            Verdict::Rebuilt(why) => rebuilt.push((key, total, why)),
        }
    }
    let by_files = |a: &Total, b: &Total| b.files.cmp(&a.files).then(b.lost.cmp(&a.lost));
    new.sort_by(|a, b| by_files(a.1, b.1));
    known.sort_by(|a, b| by_files(a.1, b.1));

    let mut out = String::new();
    let audited = files.len().saturating_sub(unreadable.len());
    let _ = writeln!(out, "Saved {audited} GP7 file(s) again and compared them.");
    for line in &unreadable {
        let _ = writeln!(out, "  not audited: {line}");
    }
    let row = |out: &mut String, key: &str, total: &Total| {
        let _ = write!(
            out,
            "  {:>8} in {:>3} file(s)  {key}",
            total.lost, total.files
        );
    };
    let _ = writeln!(out, "\nNew losses, not looked at yet: {}", new.len());
    for (key, total) in &new {
        row(&mut out, key, total);
        let example = total
            .example
            .as_deref()
            .map(|file| file.display().to_string())
            .unwrap_or_default();
        let _ = writeln!(out, "  (e.g. {example})");
    }
    let _ = writeln!(out, "\nKnown losses, still to be fixed: {}", known.len());
    for (key, total, why) in &known {
        row(&mut out, key, total);
        let _ = writeln!(out, ": {why}");
    }
    let _ = writeln!(
        out,
        "\nRebuilt by Guitar Pro (nothing lost): {} kind(s){}",
        rebuilt.len(),
        if verbose {
            ""
        } else {
            " (--verbose lists them)"
        }
    );
    if verbose {
        for (key, total, why) in &rebuilt {
            row(&mut out, key, total);
            let _ = writeln!(out, ": {why}");
        }
    }
    if check && (!new.is_empty() || !unreadable.is_empty()) {
        return Err(CliError::Failed(format!(
            "{out}\nThe audit failed: {} new loss(es), {} file(s) not audited. Fix the save, \
             or give each key its verdict in gscore-io's audit.",
            new.len(),
            unreadable.len()
        )));
    }
    Ok(out)
}

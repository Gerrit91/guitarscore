//! `cargo xtask layers`: enforces the crate architecture from AGENTS.md section 5.
//!
//! Checks, collecting every violation before failing:
//! 1. every workspace crate is registered in [`LAYERS`];
//! 2. a crate depends only on workspace crates in strictly lower layers;
//! 3. restricted third-party crates (GUI, audio devices, `guitarpro`) are used only where
//!    allowed;
//! 4. every crate opts into `[workspace.lints]`, the core crates avoid std APIs that do not
//!    work on wasm, and restricted modules (`gscore_model::interop`) are used only where
//!    allowed (see [`crate::source_scan`]);
//! 5. the core crates build for `wasm32-unknown-unknown` (see [`crate::wasm`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::{cmd, source_scan, wasm, TaskResult};

/// Layer of every workspace crate, lowest first. Register new crates here.
pub(crate) const LAYERS: &[(&str, u8)] = &[
    ("gscore-model", 0),
    ("gscore-io", 1),
    ("gscore-layout", 1),
    ("gscore-play", 1),
    ("gscore-edit", 2),
    ("gscore-audio", 2),
    ("gscore-session", 3),
    ("gscore-cli", 4),
    ("gscore-app", 4),
];

/// Core crates that must compile for `wasm32-unknown-unknown` and stay free of fs/thread/time.
pub(crate) const WASM_CRATES: &[&str] = &[
    "gscore-model",
    "gscore-io",
    "gscore-layout",
    "gscore-play",
    "gscore-edit",
];

/// Workspace members that are tools, not part of the layered application.
const TOOLS: &[&str] = &["xtask"];

/// Third-party crates that only the listed workspace crates may depend on directly:
/// (allowed crates, restricted dependencies).
const RESTRICTED: &[(&[&str], &[&str])] = &[
    (&["gscore-app"], &["egui", "eframe", "winit", "rfd"]),
    (&["gscore-audio"], &["cpal", "midir", "rustysynth"]),
    // Everyone else goes through gscore-model's wrapper (AGENTS.md section 5, decision 1).
    (&["gscore-model", "gscore-io"], &["guitarpro"]),
];

/// GUI crates that no crate below `gscore-app` may reach, even transitively.
const GUI_CRATES: &[&str] = &["egui", "eframe", "winit", "rfd"];

/// Dependency names (direct, and full transitive closure) of one crate.
#[derive(Debug, Default)]
struct Deps {
    direct: BTreeSet<String>,
    transitive: BTreeSet<String>,
}

pub(crate) fn run(root: &Path, skip_wasm: bool) -> TaskResult {
    let members = workspace_members(root)?;
    let mut deps = BTreeMap::new();
    for member in &members {
        deps.insert(member.clone(), crate_deps(root, member)?);
    }

    let mut errors = check_graph(&members, &deps);
    errors.extend(source_scan::check_manifests(root, &members));
    errors.extend(source_scan::check_wasm_std_usage(root, WASM_CRATES));
    // Tools (xtask) mention restricted modules in their own test data; skip them.
    let app_members: Vec<String> = members
        .iter()
        .filter(|m| !TOOLS.contains(&m.as_str()))
        .cloned()
        .collect();
    errors.extend(source_scan::check_restricted_modules(root, &app_members));
    if !errors.is_empty() {
        return Err(format!(
            "layers check found {} problem(s):\n  - {}",
            errors.len(),
            errors.join("\n  - ")
        ));
    }
    println!("layers: dependency graph, lints and std usage OK");

    wasm::check(root, WASM_CRATES, skip_wasm)
}

fn layer_of(name: &str) -> Option<u8> {
    LAYERS
        .iter()
        .find(|(crate_name, _)| *crate_name == name)
        .map(|(_, layer)| *layer)
}

/// Pure check of the dependency graph; returns one message per violation.
fn check_graph(members: &[String], deps: &BTreeMap<String, Deps>) -> Vec<String> {
    let mut errors = Vec::new();
    for member in members {
        if TOOLS.contains(&member.as_str()) {
            continue;
        }
        let Some(layer) = layer_of(member) else {
            errors.push(format!(
                "`{member}` is not registered in LAYERS (xtask/src/layers.rs)"
            ));
            continue;
        };
        let Some(member_deps) = deps.get(member) else {
            continue;
        };
        for dep in &member_deps.direct {
            if TOOLS.contains(&dep.as_str()) {
                errors.push(format!("`{member}` depends on the tool crate `{dep}`"));
            }
            if let Some(dep_layer) = layer_of(dep) {
                if dep_layer >= layer {
                    errors.push(format!(
                        "`{member}` (layer {layer}) depends on `{dep}` (layer {dep_layer}); \
                         only lower layers are allowed"
                    ));
                }
            }
            for (allowed, restricted) in RESTRICTED {
                if restricted.contains(&dep.as_str()) && !allowed.contains(&member.as_str()) {
                    errors.push(format!(
                        "`{member}` uses `{dep}`, which only {} may use",
                        allowed.join(", ")
                    ));
                }
            }
        }
        if member != "gscore-app" {
            for gui in GUI_CRATES {
                if member_deps.transitive.contains(*gui) {
                    errors.push(format!(
                        "`{member}` pulls in the GUI crate `{gui}` (only gscore-app may)"
                    ));
                }
            }
        }
    }
    errors
}

/// Names of all workspace members.
fn workspace_members(root: &Path) -> Result<Vec<String>, String> {
    let out = cmd::run_capture(
        &cmd::cargo(),
        &[
            "tree",
            "--workspace",
            "--depth",
            "0",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ],
        root,
    )?;
    let members: BTreeSet<String> = package_names(&out).collect();
    Ok(members.into_iter().collect())
}

/// Normal and build dependencies of `member` on every target platform.
fn crate_deps(root: &Path, member: &str) -> Result<Deps, String> {
    let tree = |depth: Option<&str>| -> Result<BTreeSet<String>, String> {
        let mut args = vec![
            "tree",
            "--package",
            member,
            "--edges",
            "normal,build",
            "--target",
            "all",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ];
        if let Some(depth) = depth {
            args.extend(["--depth", depth]);
        }
        let out = cmd::run_capture(&cmd::cargo(), &args, root)?;
        // The first line is the package itself.
        Ok(package_names(&out).skip(1).collect())
    };
    Ok(Deps {
        direct: tree(Some("1"))?,
        transitive: tree(None)?,
    })
}

/// Package names from `cargo tree --prefix none --format {p}` output
/// (lines like `gscore-model v0.1.0 (/path)`; repeated subtrees end in `(*)`).
fn package_names(output: &str) -> impl Iterator<Item = String> + '_ {
    output
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(edges: &[(&str, &[&str])]) -> (Vec<String>, BTreeMap<String, Deps>) {
        let members = edges.iter().map(|(m, _)| m.to_string()).collect();
        let deps = edges
            .iter()
            .map(|(m, ds)| {
                let set: BTreeSet<String> = ds.iter().map(|d| d.to_string()).collect();
                (
                    m.to_string(),
                    Deps {
                        direct: set.clone(),
                        transitive: set,
                    },
                )
            })
            .collect();
        (members, deps)
    }

    #[test]
    fn accepts_downward_dependencies() {
        let (m, d) = graph(&[
            ("gscore-model", &[]),
            ("gscore-io", &["gscore-model", "guitarpro"]),
            ("gscore-app", &["gscore-io", "egui"]),
            ("xtask", &[]),
        ]);
        assert_eq!(check_graph(&m, &d), Vec::<String>::new());
    }

    #[test]
    fn rejects_same_layer_and_upward_dependencies() {
        let (m, d) = graph(&[
            ("gscore-model", &["gscore-io"]),
            ("gscore-app", &["gscore-cli"]),
        ]);
        assert_eq!(check_graph(&m, &d).len(), 2);
    }

    #[test]
    fn rejects_unregistered_crates_and_restricted_deps() {
        let (m, d) = graph(&[
            ("gscore-new", &[]),
            ("gscore-play", &["cpal", "egui", "guitarpro"]),
            ("gscore-io", &["guitarpro"]),
        ]);
        // unregistered; cpal, egui and guitarpro not allowed in play; egui reached below app
        assert_eq!(check_graph(&m, &d).len(), 5);
    }

    #[test]
    fn parses_cargo_tree_output() {
        let out = "gscore-io v0.1.0 (/x)\nguitarpro v0.4.4 (https://y#z)\n\nzip v9.0.5 (*)\n";
        let names: Vec<String> = package_names(out).collect();
        assert_eq!(names, ["gscore-io", "guitarpro", "zip"]);
    }
}

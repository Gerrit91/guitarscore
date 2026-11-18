//! Guitar Pro files opening with GuitarScore (a double click, "Open with"), for the user
//! only and without an installer. Off until switched on in Settings › Editor.
//!
//! - Linux: a file type for the Guitar Pro extensions in the user's data folder
//!   (`mime/packages/guitarscore.xml`) and a hidden desktop entry that opens it
//!   (`applications/guitarscore-files.desktop`), apart from the menu entry, so files open
//!   with GuitarScore whether it is in the applications menu or not. Being the only program
//!   for the type, GuitarScore is its default.
//! - Windows: a program id under `HKEY_CURRENT_USER\Software\Classes` and the id listed in
//!   each extension's `OpenWithProgids`, written with `reg.exe` (the workspace forbids the
//!   `unsafe` a direct registry call needs). Windows lets only the user choose the default
//!   program, so GuitarScore is in "Open with", and "Always" there makes it the default.
//!
//! Like the menu entry, the association names the program by its absolute path:
//! [`FileTypes::refresh`] rewrites it when the program moved.

use std::io;
use std::path::{Path, PathBuf};

use crate::desktop_entry::{self, DesktopEntry};

/// The extensions GuitarScore opens.
pub(crate) const EXTENSIONS: [&str; 5] = ["gp", "gpx", "gp5", "gp4", "gp3"];

/// Linux: the file type of every Guitar Pro file.
const MIME_TYPE: &str = "application/x-guitar-pro";
/// Linux: the hidden desktop entry that opens the files.
const FILES_ENTRY: &str = "guitarscore-files.desktop";
/// Linux: the file type's definition, in the data folder.
const MIME_PACKAGE: &str = "mime/packages/guitarscore.xml";

/// Windows: the program id the extensions point to.
const PROG_ID: &str = "GuitarScore.Song";
/// Windows: the user's own file classes (no administrator rights needed).
const CLASSES: &str = r"HKCU\Software\Classes";

/// Where the association is written and which program it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileTypes {
    /// The user's data folder and the program, as for the menu entry.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Linux(DesktopEntry),
    /// The user's registry and this executable.
    #[cfg_attr(not(windows), allow(dead_code))]
    Windows { program: PathBuf },
}

impl FileTypes {
    /// The association for this user and program, where the app offers one.
    #[cfg(target_os = "linux")]
    pub(crate) fn for_this_program() -> Option<Self> {
        DesktopEntry::for_this_program().map(FileTypes::Linux)
    }

    /// The association for this user and program, where the app offers one.
    #[cfg(windows)]
    pub(crate) fn for_this_program() -> Option<Self> {
        std::env::current_exe()
            .ok()
            .map(|program| FileTypes::Windows { program })
    }

    /// Whether an association this app wrote is there.
    pub(crate) fn is_installed(&self) -> bool {
        match self {
            FileTypes::Linux(entry) => read_ours(&files_entry_path(entry)).is_some(),
            FileTypes::Windows { .. } => registered_command().is_some(),
        }
    }

    /// Makes the Guitar Pro files open with this program.
    pub(crate) fn install(&self) -> io::Result<()> {
        match self {
            FileTypes::Linux(entry) => {
                entry.install_icons()?;
                desktop_entry::write_file(
                    &entry.data_home().join(MIME_PACKAGE),
                    mime_package().as_bytes(),
                )?;
                desktop_entry::write_file(
                    &files_entry_path(entry),
                    files_entry_text(entry.program()).as_bytes(),
                )?;
                update_databases(entry.data_home());
                Ok(())
            }
            FileTypes::Windows { program } => {
                for args in install_commands(program) {
                    run_reg(&args)?;
                }
                Ok(())
            }
        }
    }

    /// Undoes [`FileTypes::install`] (only what this app wrote).
    pub(crate) fn remove(&self) -> io::Result<()> {
        match self {
            FileTypes::Linux(entry) => {
                if read_ours(&files_entry_path(entry)).is_none() {
                    return Ok(());
                }
                std::fs::remove_file(files_entry_path(entry))?;
                remove_if_there(&entry.data_home().join(MIME_PACKAGE))?;
                // The icons stay while the menu entry uses them.
                if !entry.is_installed() {
                    entry.remove_icons()?;
                }
                update_databases(entry.data_home());
                Ok(())
            }
            FileTypes::Windows { .. } => {
                if registered_command().is_none() {
                    return Ok(());
                }
                for args in remove_commands() {
                    // An extension that lost its entry some other way is no error.
                    let _ = run_reg(&args);
                }
                Ok(())
            }
        }
    }

    /// Brings an association of ours up to date with this program (it moved, or the app
    /// was updated); without one nothing is written.
    pub(crate) fn refresh(&self) -> io::Result<()> {
        match self {
            FileTypes::Linux(entry) => {
                let Some(existing) = read_ours(&files_entry_path(entry)) else {
                    return Ok(());
                };
                let package = std::fs::read_to_string(entry.data_home().join(MIME_PACKAGE));
                if existing != files_entry_text(entry.program())
                    || package.ok().as_deref() != Some(mime_package().as_str())
                    || !entry.icons_current()
                {
                    self.install()?;
                }
                Ok(())
            }
            FileTypes::Windows { program } => match registered_command() {
                Some(command) if command.contains(&open_command(program)) => Ok(()),
                Some(_) => self.install(),
                None => Ok(()),
            },
        }
    }
}

fn files_entry_path(entry: &DesktopEntry) -> PathBuf {
    entry.data_home().join("applications").join(FILES_ENTRY)
}

/// The file's text, if it is there and this app wrote it.
fn read_ours(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .filter(|text| desktop_entry::is_ours(text))
}

fn remove_if_there(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The hidden entry that opens the files: not in the menu (that is the other entry's
/// job), only offered for the file type.
fn files_entry_text(program: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=GuitarScore\n\
         Comment=Open Guitar Pro files with GuitarScore\n\
         Exec={} %f\n\
         Icon=guitarscore\n\
         Terminal=false\n\
         NoDisplay=true\n\
         MimeType={MIME_TYPE};\n\
         StartupWMClass=guitarscore\n\
         {}\n",
        desktop_entry::exec_argument(program),
        desktop_entry::MARKER,
    )
}

/// The file type's definition: every Guitar Pro extension, weighted above the default
/// (50) so a `.gp` is a Guitar Pro file rather than another program's that uses the same
/// extension.
fn mime_package() -> String {
    let globs: String = EXTENSIONS
        .iter()
        .map(|extension| format!("    <glob pattern=\"*.{extension}\" weight=\"60\"/>\n"))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!-- Written by GuitarScore ({}); removed when its file association is switched off. -->\n\
         <mime-info xmlns=\"http://www.freedesktop.org/standards/shared-mime-info\">\n\
         \x20 <mime-type type=\"{MIME_TYPE}\">\n\
         \x20   <comment>Guitar Pro tablature</comment>\n\
         \x20   <generic-icon name=\"audio-x-generic\"/>\n\
         {globs}\
         \x20 </mime-type>\n\
         </mime-info>\n",
        desktop_entry::MARKER,
    )
}

/// Lets the desktop see the new type and entry at once (without, it does at the next
/// login). The tools are part of every desktop; a missing one is no error.
fn update_databases(data_home: &Path) {
    for (tool, dir) in [
        ("update-mime-database", data_home.join("mime")),
        ("update-desktop-database", data_home.join("applications")),
    ] {
        if !dir.is_dir() {
            continue;
        }
        match std::process::Command::new(tool).arg(&dir).output() {
            Ok(output) if !output.status.success() => {
                eprintln!(
                    "guitarscore: {tool} failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("guitarscore: cannot run {tool}: {error}"),
        }
    }
}

/// Windows: the command the program id opens a file with.
fn open_command(program: &Path) -> String {
    format!("\"{}\" \"%1\"", program.display())
}

/// Windows: the `reg.exe` calls that register the program id and list it for every
/// extension.
fn install_commands(program: &Path) -> Vec<Vec<String>> {
    let key = format!(r"{CLASSES}\{PROG_ID}");
    let set = |key: String, value: String| {
        vec![
            "add".to_owned(),
            key,
            "/ve".to_owned(),
            "/t".to_owned(),
            "REG_SZ".to_owned(),
            "/d".to_owned(),
            value,
            "/f".to_owned(),
        ]
    };
    let mut commands = vec![
        set(key.clone(), "Guitar Pro tablature".to_owned()),
        set(
            format!(r"{key}\DefaultIcon"),
            format!("\"{}\",0", program.display()),
        ),
        set(format!(r"{key}\shell\open\command"), open_command(program)),
    ];
    for extension in EXTENSIONS {
        commands.push(vec![
            "add".to_owned(),
            format!(r"{CLASSES}\.{extension}\OpenWithProgids"),
            "/v".to_owned(),
            PROG_ID.to_owned(),
            "/t".to_owned(),
            "REG_NONE".to_owned(),
            "/f".to_owned(),
        ]);
    }
    commands
}

/// Windows: the `reg.exe` calls that undo [`install_commands`].
fn remove_commands() -> Vec<Vec<String>> {
    let mut commands: Vec<Vec<String>> = EXTENSIONS
        .iter()
        .map(|extension| {
            vec![
                "delete".to_owned(),
                format!(r"{CLASSES}\.{extension}\OpenWithProgids"),
                "/v".to_owned(),
                PROG_ID.to_owned(),
                "/f".to_owned(),
            ]
        })
        .collect();
    commands.push(vec![
        "delete".to_owned(),
        format!(r"{CLASSES}\{PROG_ID}"),
        "/f".to_owned(),
    ]);
    commands
}

/// Windows: the registered open command (`reg query`'s output for it), if there is one.
fn registered_command() -> Option<String> {
    let args = [
        "query".to_owned(),
        format!(r"{CLASSES}\{PROG_ID}\shell\open\command"),
        "/ve".to_owned(),
    ];
    run_reg(&args).ok()
}

/// Runs `reg.exe` with `args` (without a console window flashing up), returning its
/// output.
#[cfg(windows)]
fn run_reg(args: &[String]) -> io::Result<String> {
    use std::os::windows::process::CommandExt as _;
    /// `CREATE_NO_WINDOW`: a GUI program's child gets no console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("reg")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

/// There is no registry here.
#[cfg(not(windows))]
fn run_reg(_args: &[String]) -> io::Result<String> {
    Err(io::Error::other("the registry exists on Windows only"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gscore-types-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn linux(home: &Path, program: &str) -> FileTypes {
        FileTypes::Linux(DesktopEntry::new(home.to_path_buf(), PathBuf::from(program)))
    }

    #[test]
    fn linux_gets_the_file_type_and_its_hidden_entry_and_loses_them_again() {
        let home = temp_dir("linux");
        let types = linux(&home, "/opt/gs/guitarscore");
        assert!(!types.is_installed());
        types.install().unwrap();
        assert!(types.is_installed());
        let entry = std::fs::read_to_string(home.join("applications").join(FILES_ENTRY)).unwrap();
        assert!(entry.contains("\nExec=\"/opt/gs/guitarscore\" %f\n"));
        assert!(entry.contains("\nMimeType=application/x-guitar-pro;\n"));
        assert!(entry.contains("\nNoDisplay=true\n"), "not a second menu entry");
        let package = std::fs::read_to_string(home.join(MIME_PACKAGE)).unwrap();
        for extension in EXTENSIONS {
            assert!(package.contains(&format!("pattern=\"*.{extension}\"")));
        }
        // The specification's own checker, where it is installed (desktop-file-utils).
        if let Ok(output) = std::process::Command::new("desktop-file-validate")
            .arg(home.join("applications").join(FILES_ENTRY))
            .output()
        {
            assert!(
                output.status.success() && output.stdout.is_empty(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
        let icon = home.join("icons/hicolor/256x256/apps/guitarscore.png");
        assert!(icon.exists(), "the entry's icon is there");

        types.remove().unwrap();
        assert!(!types.is_installed());
        assert!(!home.join(MIME_PACKAGE).exists());
        assert!(!icon.exists(), "no menu entry needs the icon");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_menu_entry_keeps_its_icon_when_the_association_goes() {
        let home = temp_dir("icon");
        let menu = DesktopEntry::new(home.clone(), PathBuf::from("/opt/guitarscore"));
        menu.install().unwrap();
        let types = linux(&home, "/opt/guitarscore");
        types.install().unwrap();
        types.remove().unwrap();
        assert!(menu.is_installed());
        assert!(home.join("icons/hicolor/256x256/apps/guitarscore.png").exists());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_moved_program_gets_its_association_rewritten_and_none_is_made_unasked() {
        let home = temp_dir("refresh");
        linux(&home, "/new/guitarscore").refresh().unwrap();
        assert!(!linux(&home, "/new/guitarscore").is_installed());
        linux(&home, "/old/guitarscore").install().unwrap();
        linux(&home, "/new/guitarscore").refresh().unwrap();
        let entry = std::fs::read_to_string(home.join("applications").join(FILES_ENTRY)).unwrap();
        assert!(entry.contains("Exec=\"/new/guitarscore\" %f"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn windows_registers_the_program_id_for_every_extension_and_undoes_it() {
        let program = Path::new(r"C:\Users\a b\GuitarScore\guitarscore.exe");
        let commands = install_commands(program);
        let command = commands
            .iter()
            .find(|args| args.get(1).is_some_and(|key| key.ends_with(r"\shell\open\command")))
            .unwrap();
        assert_eq!(
            command,
            &[
                "add",
                r"HKCU\Software\Classes\GuitarScore.Song\shell\open\command",
                "/ve",
                "/t",
                "REG_SZ",
                "/d",
                r#""C:\Users\a b\GuitarScore\guitarscore.exe" "%1""#,
                "/f",
            ]
        );
        for extension in EXTENSIONS {
            let key = format!(r"HKCU\Software\Classes\.{extension}\OpenWithProgids");
            assert!(commands.iter().any(|args| args.get(1) == Some(&key)
                && args.contains(&"GuitarScore.Song".to_owned())));
            assert!(remove_commands().iter().any(|args| args.get(1) == Some(&key)
                && args.first().map(String::as_str) == Some("delete")));
        }
        // Only the user's own classes: no administrator rights, nothing for other users.
        assert!(
            commands
                .iter()
                .chain(&remove_commands())
                .all(|args| args.get(1).is_some_and(|key| key.starts_with(r"HKCU\Software\Classes\")))
        );
    }
}

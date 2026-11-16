//! The app in the Linux applications menu: a desktop entry and the icon in the user's data
//! folder (`~/.local/share`, no root rights needed).
//!
//! Linux has no icon inside an executable. Desktops find an app's name and icon through
//! its desktop entry, and on Wayland the dock and the task switcher match the window to it
//! by the app id (`guitarscore`, `StartupWMClass`): without an entry the app shows a
//! generic icon and cannot be pinned. A release archive or AppImage installs nothing, so
//! the app offers to write the entry itself.
//!
//! The entry names the program by its absolute path, which changes when the folder is
//! moved or replaced by an update (and is the `.AppImage` file, not its temporary mount).
//! [`DesktopEntry::refresh`] rewrites an entry this app wrote, so the menu keeps working.

use std::io;
use std::path::{Path, PathBuf};

/// The desktop entry's file name; its id matches the window's app id.
const ENTRY_FILE: &str = "guitarscore.desktop";
/// Marks an entry the app wrote (and may rewrite or remove).
const MARKER: &str = "X-GuitarScore-Managed=true";
/// The icon in the size most desktops use, and drawn for any size.
const PNG: &[u8] = crate::APP_ICON;
const SVG: &[u8] = include_bytes!("../../../packaging/guitarscore.svg");

/// Where the entry goes and which program it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesktopEntry {
    /// The user's data folder (`$XDG_DATA_HOME`, else `~/.local/share`).
    data_home: PathBuf,
    /// The program to start: the `.AppImage` file, else this executable.
    program: PathBuf,
}

/// What [`DesktopEntry::refresh`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refresh {
    /// There is no entry (never added, or removed by the user): nothing is written.
    Missing,
    /// Someone else's entry with the same name (a distribution package): left alone.
    NotOurs,
    /// Ours, and already right.
    Current,
    /// Ours, and rewritten (the program moved, or the entry changed with an update).
    Updated,
}

impl DesktopEntry {
    /// The entry for this user and this program, if the data folder and the program are
    /// known.
    pub(crate) fn for_this_program() -> Option<Self> {
        let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
        let data_home = var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".local/share")))?;
        // An AppImage runs from a new temporary mount each time; it names its own file.
        let program = var("APPIMAGE")
            .map(PathBuf::from)
            .filter(|file| file.is_absolute() && file.is_file())
            .or_else(|| std::env::current_exe().ok())?;
        Some(Self::new(data_home, program))
    }

    /// The entry in `data_home` starting `program`.
    pub(crate) fn new(data_home: PathBuf, program: PathBuf) -> Self {
        DesktopEntry { data_home, program }
    }

    fn entry_path(&self) -> PathBuf {
        self.data_home.join("applications").join(ENTRY_FILE)
    }

    fn icon_paths(&self) -> [(PathBuf, &'static [u8]); 2] {
        let icons = self.data_home.join("icons/hicolor");
        [
            (icons.join("256x256/apps/guitarscore.png"), PNG),
            (icons.join("scalable/apps/guitarscore.svg"), SVG),
        ]
    }

    /// Whether an entry this app wrote is there.
    pub(crate) fn is_installed(&self) -> bool {
        self.read_ours().is_some()
    }

    /// Writes the entry and the icons (over an entry of ours; over someone else's too,
    /// since the user asked for it).
    pub(crate) fn install(&self) -> io::Result<()> {
        for (path, bytes) in self.icon_paths() {
            write_file(&path, bytes)?;
        }
        write_file(&self.entry_path(), self.text().as_bytes())
    }

    /// Removes the entry and the icons, if the entry is ours.
    pub(crate) fn remove(&self) -> io::Result<()> {
        if self.read_ours().is_none() {
            return Ok(());
        }
        std::fs::remove_file(self.entry_path())?;
        for (path, _) in self.icon_paths() {
            match std::fs::remove_file(path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        Ok(())
    }

    /// Brings an entry of ours up to date with this program; anything else stays as it
    /// is.
    pub(crate) fn refresh(&self) -> io::Result<Refresh> {
        let existing = match std::fs::read_to_string(self.entry_path()) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Refresh::Missing),
            Err(error) => return Err(error),
        };
        if !is_ours(&existing) {
            return Ok(Refresh::NotOurs);
        }
        let icons_current = self
            .icon_paths()
            .iter()
            .all(|(path, bytes)| std::fs::read(path).is_ok_and(|have| have == *bytes));
        if existing == self.text() && icons_current {
            return Ok(Refresh::Current);
        }
        self.install()?;
        Ok(Refresh::Updated)
    }

    fn read_ours(&self) -> Option<String> {
        std::fs::read_to_string(self.entry_path())
            .ok()
            .filter(|text| is_ours(text))
    }

    /// The desktop entry's text.
    fn text(&self) -> String {
        format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=GuitarScore\n\
             GenericName=Guitar Tablature Editor\n\
             Comment=Write, edit and play guitar tablature\n\
             Exec={} %f\n\
             Icon=guitarscore\n\
             Terminal=false\n\
             Categories=AudioVideo;Music;\n\
             StartupNotify=true\n\
             StartupWMClass=guitarscore\n\
             {MARKER}\n",
            exec_argument(&self.program)
        )
    }
}

fn is_ours(text: &str) -> bool {
    text.lines().any(|line| line.trim() == MARKER)
}

/// `path` as the program in an `Exec` line: quoted, with the characters the desktop entry
/// specification reserves inside quotes escaped, then escaped again as a string value
/// (backslashes doubled) and with `%` doubled (field codes).
fn exec_argument(path: &Path) -> String {
    let mut quoted = String::from('"');
    for c in path.to_string_lossy().chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted.replace('\\', "\\\\").replace('%', "%%")
}

/// Writes `bytes` to `path` through a temporary file and a rename, creating the folder.
fn write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("no folder for the file"))?;
    std::fs::create_dir_all(dir)?;
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gscore-desktop-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn the_entry_and_icons_are_installed_and_removed() {
        let home = temp_dir("install");
        let entry = DesktopEntry::new(home.clone(), PathBuf::from("/opt/gs/guitarscore"));
        assert!(!entry.is_installed());
        entry.install().unwrap();
        assert!(entry.is_installed());
        let text = read(&home.join("applications/guitarscore.desktop"));
        assert!(text.contains("\nExec=\"/opt/gs/guitarscore\" %f\n"));
        assert!(text.contains("\nIcon=guitarscore\n"));
        assert!(text.contains("\nStartupWMClass=guitarscore\n"));
        // The specification's own checker, where it is installed (desktop-file-utils).
        if let Ok(output) = std::process::Command::new("desktop-file-validate")
            .arg(home.join("applications/guitarscore.desktop"))
            .output()
        {
            assert!(
                output.status.success() && output.stdout.is_empty(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
        let png = home.join("icons/hicolor/256x256/apps/guitarscore.png");
        let svg = home.join("icons/hicolor/scalable/apps/guitarscore.svg");
        assert_eq!(std::fs::read(&png).unwrap(), PNG);
        assert_eq!(std::fs::read(&svg).unwrap(), SVG);

        entry.remove().unwrap();
        assert!(!entry.is_installed());
        assert!(!home.join("applications/guitarscore.desktop").exists());
        assert!(!png.exists() && !svg.exists());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_moved_program_gets_its_entry_rewritten() {
        let home = temp_dir("refresh");
        let old = DesktopEntry::new(home.clone(), PathBuf::from("/old/guitarscore"));
        let new = DesktopEntry::new(home.clone(), PathBuf::from("/new/guitarscore"));
        assert_eq!(new.refresh().unwrap(), Refresh::Missing);
        assert!(!new.is_installed(), "no entry is made without a yes");

        old.install().unwrap();
        assert_eq!(new.refresh().unwrap(), Refresh::Updated);
        let text = read(&home.join("applications/guitarscore.desktop"));
        assert!(text.contains("Exec=\"/new/guitarscore\" %f"));
        assert_eq!(new.refresh().unwrap(), Refresh::Current);

        // A missing icon is put back.
        std::fs::remove_file(home.join("icons/hicolor/scalable/apps/guitarscore.svg")).unwrap();
        assert_eq!(new.refresh().unwrap(), Refresh::Updated);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn someone_elses_entry_is_left_alone() {
        let home = temp_dir("foreign");
        let path = home.join("applications/guitarscore.desktop");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let foreign = "[Desktop Entry]\nType=Application\nName=GuitarScore\nExec=guitarscore\n";
        std::fs::write(&path, foreign).unwrap();
        let entry = DesktopEntry::new(home.clone(), PathBuf::from("/opt/guitarscore"));
        assert_eq!(entry.refresh().unwrap(), Refresh::NotOurs);
        assert!(!entry.is_installed());
        entry.remove().unwrap();
        assert_eq!(read(&path), foreign);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn odd_characters_in_the_path_are_escaped() {
        assert_eq!(
            exec_argument(Path::new("/home/a b/GuitarScore 100%/guitarscore")),
            "\"/home/a b/GuitarScore 100%%/guitarscore\""
        );
        // `"` `$` and a backslash: escaped inside the quotes, then the backslashes doubled.
        assert_eq!(
            exec_argument(Path::new("/x/\"$\\/g")),
            "\"/x/\\\\\"\\\\$\\\\\\\\/g\""
        );
    }
}

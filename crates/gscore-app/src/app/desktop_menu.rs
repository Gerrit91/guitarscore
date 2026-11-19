//! The app in the Linux applications menu: the first-start question, the switch in
//! Settings › Editor, and keeping an entry the app added pointing at the running program
//! (see [`crate::desktop_entry`]). Also the Guitar Pro files opening with the app, on
//! Linux and Windows: only a switch in Settings › Editor (see [`crate::file_types`]).

use egui::RichText;

use super::GuitarScoreApp;
use super::alert::{AlertIcon, Answer, Reply, alert};
use crate::desktop_entry::{DesktopEntry, Refresh};
use crate::file_types::FileTypes;

/// The applications menu's state while the app runs.
#[derive(Debug, Default)]
pub(super) struct DesktopMenu {
    /// The entry this program would write; `None` where the app does not offer one (not
    /// Linux, or a build from a source checkout).
    entry: Option<DesktopEntry>,
    /// The first-start question was answered or dismissed in this run.
    asked: bool,
    /// The file association this program would write; `None` where the app does not
    /// offer one (not Linux or Windows, or a build from a source checkout).
    file_types: Option<FileTypes>,
}

impl GuitarScoreApp {
    /// Offers the applications menu entry (asked once) and brings an entry the app added
    /// up to date with this program. The Linux release binary calls this.
    pub fn enable_desktop_entry(&mut self) {
        if let Some(entry) = DesktopEntry::for_this_program() {
            self.use_desktop_entry(entry);
        }
    }

    /// [`GuitarScoreApp::enable_desktop_entry`] with the entry given (tests write into a
    /// folder of their own).
    pub(super) fn use_desktop_entry(&mut self, entry: DesktopEntry) {
        // The program moved (an update, a moved folder, every AppImage start): quietly
        // rewritten. An entry the user removed by hand is not brought back.
        if self.settings.desktop_entry == Some(true) {
            match entry.refresh() {
                Ok(Refresh::Missing | Refresh::NotOurs | Refresh::Current | Refresh::Updated) => {}
                Err(error) => eprintln!("guitarscore: cannot update the menu entry: {error}"),
            }
        }
        self.desktop_menu.entry = Some(entry);
    }

    /// Whether this build can add itself to the applications menu.
    pub(super) fn offers_desktop_entry(&self) -> bool {
        self.desktop_menu.entry.is_some()
    }

    /// Whether the app's entry is in the applications menu.
    pub(super) fn has_desktop_entry(&self) -> bool {
        self.desktop_menu
            .entry
            .as_ref()
            .is_some_and(DesktopEntry::is_installed)
    }

    /// Adds the entry to the applications menu or removes it, and remembers the choice.
    pub(super) fn set_desktop_entry(&mut self, on: bool) {
        let Some(entry) = &self.desktop_menu.entry else {
            return;
        };
        let result = if on { entry.install() } else { entry.remove() };
        match result {
            Ok(()) => {
                self.status = Some(
                    if on {
                        "GuitarScore is in the applications menu"
                    } else {
                        "GuitarScore was removed from the applications menu"
                    }
                    .to_owned(),
                );
            }
            Err(error) => {
                self.notice = Some(format!("Could not change the applications menu: {error}"));
            }
        }
        self.settings.desktop_entry = Some(on);
        self.save_settings();
    }

    /// Offers the Guitar Pro files' association in Settings, and brings one the app wrote
    /// up to date with this program. The Linux and Windows release binaries call this.
    #[cfg(any(target_os = "linux", windows))]
    pub fn enable_file_association(&mut self) {
        if let Some(types) = FileTypes::for_this_program() {
            self.use_file_types(types);
        }
    }

    /// [`GuitarScoreApp::enable_file_association`] with the association given (tests
    /// write into a folder of their own).
    pub(super) fn use_file_types(&mut self, types: FileTypes) {
        // The program moved: quietly rewritten, while the setting is on.
        if self.settings.file_association
            && let Err(error) = types.refresh()
        {
            eprintln!("guitarscore: cannot update the file association: {error}");
        }
        self.desktop_menu.file_types = Some(types);
    }

    /// Whether this build can make the Guitar Pro files open with it.
    pub(super) fn offers_file_association(&self) -> bool {
        self.desktop_menu.file_types.is_some()
    }

    /// Whether the Guitar Pro files open with GuitarScore.
    pub(super) fn has_file_association(&self) -> bool {
        self.desktop_menu
            .file_types
            .as_ref()
            .is_some_and(FileTypes::is_installed)
    }

    /// Makes the Guitar Pro files open with GuitarScore, or undoes it, and remembers it.
    pub(super) fn set_file_association(&mut self, on: bool) {
        let Some(types) = &self.desktop_menu.file_types else {
            return;
        };
        let result = if on { types.install() } else { types.remove() };
        match result {
            Ok(()) => {
                self.status = Some(
                    if on {
                        "Guitar Pro files now open with GuitarScore"
                    } else {
                        "Guitar Pro files no longer open with GuitarScore"
                    }
                    .to_owned(),
                );
            }
            Err(error) => {
                self.notice = Some(format!("Could not change the file association: {error}"));
            }
        }
        self.settings.file_association = on;
        self.save_settings();
    }

    /// Once a frame: the first-start question, after the one about updates.
    pub(super) fn show_desktop_menu_question(&mut self, ctx: &egui::Context) {
        let busy = self.dialog.is_some() || !self.recoverable.is_empty();
        if self.desktop_menu.entry.is_none()
            || self.settings.desktop_entry.is_some()
            || self.desktop_menu.asked
            || busy
            || self.update_question_pending()
        {
            return;
        }
        let reply = alert(
            ctx,
            "Applications menu",
            AlertIcon::Launcher,
            "Add GuitarScore to the applications menu?",
            |ui| {
                ui.label(
                    "Then it can be started from the menu and pinned to the dock or the \
                     taskbar, with its icon. It is added for you only, and kept up to date \
                     when GuitarScore is moved or updated.",
                );
                ui.add_space(4.0);
                ui.label(RichText::new("You can change this in Settings › Editor.").weak());
            },
            &[
                Answer {
                    text: "Don't add",
                    main: false,
                },
                Answer {
                    text: "Add to menu",
                    main: true,
                },
            ],
        );
        match reply {
            Reply::Open => return,
            Reply::Answer(1) => self.set_desktop_entry(true),
            Reply::Answer(_) => {
                self.settings.desktop_entry = Some(false);
                self.save_settings();
            }
            // Dismissed: asked again at the next start.
            Reply::Closed => {}
        }
        self.desktop_menu.asked = true;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::path::PathBuf;

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gscore-menu-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry_text(home: &std::path::Path) -> String {
        std::fs::read_to_string(home.join("applications/guitarscore.desktop")).unwrap()
    }

    #[test]
    fn without_an_offer_nothing_is_asked() {
        let ctx = egui::Context::default();
        let mut app = GuitarScoreApp::new(&ctx, None);
        app.show_desktop_menu_question(&ctx);
        assert!(!app.desktop_menu.asked);
        assert!(!app.offers_desktop_entry());
    }

    #[test]
    fn the_switch_adds_and_removes_the_entry() {
        let home = temp_dir("switch");
        let mut app = GuitarScoreApp::new(&egui::Context::default(), None);
        app.use_desktop_entry(DesktopEntry::new(
            home.clone(),
            "/opt/gs/guitarscore".into(),
        ));
        assert!(app.offers_desktop_entry());
        assert!(!app.has_desktop_entry(), "nothing is written before a yes");

        app.set_desktop_entry(true);
        assert!(app.has_desktop_entry());
        assert_eq!(app.settings.desktop_entry, Some(true));
        assert!(entry_text(&home).contains("Exec=\"/opt/gs/guitarscore\" %f"));

        app.set_desktop_entry(false);
        assert!(!app.has_desktop_entry());
        assert_eq!(app.settings.desktop_entry, Some(false));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn an_added_entry_follows_the_program_to_its_new_place() {
        let home = temp_dir("follow");
        DesktopEntry::new(home.clone(), "/old/guitarscore".into())
            .install()
            .unwrap();
        let mut app = GuitarScoreApp::new(&egui::Context::default(), None);
        app.settings.desktop_entry = Some(true);
        app.use_desktop_entry(DesktopEntry::new(home.clone(), "/new/guitarscore".into()));
        assert!(entry_text(&home).contains("Exec=\"/new/guitarscore\" %f"));

        // After a no, an entry (made some other way) is not touched.
        DesktopEntry::new(home.clone(), "/old/guitarscore".into())
            .install()
            .unwrap();
        let mut app = GuitarScoreApp::new(&egui::Context::default(), None);
        app.settings.desktop_entry = Some(false);
        app.use_desktop_entry(DesktopEntry::new(home.clone(), "/new/guitarscore".into()));
        assert!(entry_text(&home).contains("Exec=\"/old/guitarscore\" %f"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_question_matches_its_snapshot_and_a_yes_adds_the_entry() {
        use egui_kittest::kittest::Queryable as _;

        let _gpu = crate::test_support::lock_gpu();
        let home = temp_dir("question");
        let entry = DesktopEntry::new(home.clone(), "/opt/gs/guitarscore".into());
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 700.0))
            .wgpu()
            .build_eframe(|cc| {
                let mut app = GuitarScoreApp::new(&cc.egui_ctx, None);
                app.use_desktop_entry(entry);
                app
            });
        harness.run();
        harness.snapshot("desktop_menu_question");
        harness.get_by_label("Add to menu").click();
        harness.run();
        let app = harness.state();
        assert!(app.desktop_menu.asked);
        assert_eq!(app.settings.desktop_entry, Some(true));
        assert!(app.has_desktop_entry());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_file_association_is_only_a_setting_and_follows_the_program() {
        let home = temp_dir("types");
        let ctx = egui::Context::default();
        let mut app = GuitarScoreApp::new(&ctx, None);
        assert!(!app.offers_file_association());
        app.use_file_types(FileTypes::Linux(DesktopEntry::new(
            home.clone(),
            "/old/guitarscore".into(),
        )));
        assert!(app.offers_file_association());
        assert!(
            !app.has_file_association(),
            "nothing is written before it is switched on"
        );
        // No question at the start for it: only the menu entry's (which is not offered).
        app.show_desktop_menu_question(&ctx);
        assert!(!app.has_file_association());

        app.set_file_association(true);
        assert!(app.has_file_association());
        assert!(app.settings.file_association);

        // The program moved: the next start rewrites the association.
        let mut moved = GuitarScoreApp::new(&ctx, None);
        moved.settings.file_association = true;
        moved.use_file_types(FileTypes::Linux(DesktopEntry::new(
            home.clone(),
            "/new/guitarscore".into(),
        )));
        let entry =
            std::fs::read_to_string(home.join("applications/guitarscore-files.desktop")).unwrap();
        assert!(entry.contains("Exec=\"/new/guitarscore\" %f"));

        moved.set_file_association(false);
        assert!(!moved.has_file_association());
        assert!(!moved.settings.file_association);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_question_waits_for_the_update_question() {
        let ctx = egui::Context::default();
        let home = temp_dir("wait");
        let mut app = GuitarScoreApp::new(&ctx, None);
        app.use_desktop_entry(DesktopEntry::new(home.clone(), "/opt/guitarscore".into()));
        app.enable_automatic_update_check();
        assert!(app.update_question_pending());
        // Returns before drawing anything (a drawn question would need a frame).
        app.show_desktop_menu_question(&ctx);
        assert!(!app.desktop_menu.asked);
        assert!(!app.has_desktop_entry());
        let _ = std::fs::remove_dir_all(home);
    }
}

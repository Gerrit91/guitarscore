//! The update check in the app: at the first start the user is asked whether the app may
//! look for a new version once a day; Help › Check for updates asks GitHub at once. A
//! newer release is shown with its notes and a button that opens its download page. The
//! app never downloads or installs anything itself (owner decision, 2026-11-13).
//!
//! Only the desktop binary checks on its own (`enable_automatic_update_check`): tests,
//! examples and the snapshot tool never contact anyone unless told to.

use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};

use egui::RichText;

use super::GuitarScoreApp;
use super::alert::{AlertIcon, Answer, Reply, alert};
use crate::update::{self, Release};

/// The update check's state while the app runs.
#[derive(Debug, Default)]
pub(super) struct Updates {
    /// Whether the app may ask at the first start and check once a day on its own.
    automatic: bool,
    /// The first-start question was answered or dismissed in this run.
    asked: bool,
    /// The automatic check of this run was started (or was not due).
    auto_started: bool,
    /// The check that is running, if any.
    running: Option<Running>,
    /// A newer release to tell the user about, and whether they asked for the check.
    found: Option<(Release, bool)>,
    /// This build's version (another one in tests, so snapshots do not follow the commit).
    current: Current,
    /// A newer release given by a test instead of GitHub.
    #[cfg(test)]
    pub(super) fake_answer: Option<Result<String, String>>,
}

/// This build's version.
#[derive(Debug)]
struct Current(&'static str);

impl Default for Current {
    fn default() -> Self {
        Current(gscore_version::VERSION)
    }
}

/// A check on its own thread.
#[derive(Debug)]
struct Running {
    answer: Receiver<Result<String, String>>,
    /// Whether the user asked for it (then "up to date" and errors are shown too).
    manual: bool,
}

impl GuitarScoreApp {
    /// Lets the app ask the user, at the first start, whether to check for updates, and
    /// check once a day after a yes. The desktop binary calls this.
    pub fn enable_automatic_update_check(&mut self) {
        self.updates.automatic = true;
    }

    /// Whether an update check is running.
    pub(super) fn checking_for_updates(&self) -> bool {
        self.updates.running.is_some()
    }

    /// Asks GitHub for the latest release now (Help › Check for updates).
    pub(super) fn check_for_updates(&mut self, ctx: &egui::Context) {
        self.start_update_check(ctx, true);
    }

    fn start_update_check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.updates.running.is_some() {
            return;
        }
        let (sender, answer) = sync_channel(1);
        #[cfg(test)]
        if let Some(fake) = self.updates.fake_answer.clone() {
            let _ = sender.send(fake);
            self.updates.running = Some(Running { answer, manual });
            return;
        }
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("update-check".to_owned())
            .spawn(move || {
                // A send fails only once the app is gone; nobody is left to tell.
                let _ = sender.send(update::fetch_latest());
                repaint.request_repaint();
            });
        match spawned {
            Ok(_) => self.updates.running = Some(Running { answer, manual }),
            Err(error) if manual => {
                self.notice = Some(format!("Could not check for updates: {error}"));
            }
            Err(_) => {}
        }
    }

    /// Once a frame: the first-start question, the automatic check, the answer of a running
    /// check and the window about a newer release.
    pub(super) fn show_updates(&mut self, ctx: &egui::Context) {
        self.receive_update_answer();
        // Questions about the user's work (unsaved changes, crash recovery) go first.
        let busy = self.dialog.is_some() || !self.recoverable.is_empty();
        if self.updates.automatic && !self.updates.asked && !busy {
            match self.settings.check_for_updates {
                None => self.ask_about_updates(ctx),
                Some(true) if !self.updates.auto_started => {
                    self.updates.auto_started = true;
                    let now = update::now_seconds();
                    if update::check_due(self.settings.update_checked_at, now) {
                        self.settings.update_checked_at = Some(now);
                        self.start_update_check(ctx, false);
                    }
                }
                Some(_) => {}
            }
        }
        if !busy && self.updates.found.is_some() {
            self.show_new_release(ctx);
        }
    }

    /// Takes the running check's answer, if it has come.
    fn receive_update_answer(&mut self) {
        let Some(running) = &self.updates.running else {
            return;
        };
        let manual = running.manual;
        let answer = match running.answer.try_recv() {
            Ok(answer) => answer,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("the check stopped unexpectedly".to_owned()),
        };
        self.updates.running = None;
        let current = self.updates.current.0;
        match answer.and_then(|json| update::newer_release(&json, current)) {
            Ok(Some(release)) => {
                let skipped = self.settings.skipped_update.as_deref() == Some(&release.tag);
                if manual || !skipped {
                    self.updates.found = Some((release, manual));
                }
            }
            Ok(None) if manual => {
                self.status = Some(format!("GuitarScore {current} is the latest version"));
            }
            Err(error) if manual => {
                self.notice = Some(format!("Could not check for updates: {error}"));
            }
            // An automatic check stays quiet: being offline is normal.
            Ok(None) | Err(_) => {}
        }
    }

    /// The first-start question.
    fn ask_about_updates(&mut self, ctx: &egui::Context) {
        let reply = alert(
            ctx,
            "Updates",
            AlertIcon::Update,
            "Check for updates automatically?",
            |ui| {
                ui.label(
                    "Once a day, GuitarScore can ask GitHub whether a newer version is out. \
                     Nothing about you or your songs is sent.",
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "You can change this in Settings › Editor. Help › Check for updates \
                         always works.",
                    )
                    .weak(),
                );
            },
            &[
                Answer {
                    text: "Don't check",
                    main: false,
                },
                Answer {
                    text: "Check daily",
                    main: true,
                },
            ],
        );
        match reply {
            Reply::Open => return,
            Reply::Answer(index) => {
                self.settings.check_for_updates = Some(index == 1);
                self.save_settings();
            }
            // Dismissed: asked again at the next start.
            Reply::Closed => {}
        }
        self.updates.asked = true;
    }

    /// The window about a newer release: its notes, and a button to its download page.
    fn show_new_release(&mut self, ctx: &egui::Context) {
        let Some((release, _)) = &self.updates.found else {
            return;
        };
        let release = release.clone();
        let current = self.updates.current.0;
        let reply = alert(
            ctx,
            "Update available",
            AlertIcon::Update,
            &format!("{} is available", release.title),
            |ui| {
                ui.label(format!(
                    "You have {current}. The release page has the downloads for every system."
                ));
                if !release.notes.is_empty() {
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| notes(ui, &release.notes));
                }
            },
            &[
                Answer {
                    text: "Skip this version",
                    main: false,
                },
                Answer {
                    text: "Later",
                    main: false,
                },
                Answer {
                    text: "Download",
                    main: true,
                },
            ],
        );
        match reply {
            Reply::Open => return,
            Reply::Answer(0) => {
                self.settings.skipped_update = Some(release.tag);
                self.save_settings();
            }
            Reply::Answer(2) => ctx.open_url(egui::OpenUrl::new_tab(release.url)),
            Reply::Answer(_) | Reply::Closed => {}
        }
        self.updates.found = None;
    }
}

/// The release notes, Markdown as written on GitHub, lightly shown: headings in bold,
/// list items with a bullet, emphasis marks left out. Links stay as text.
fn notes(ui: &mut egui::Ui, text: &str) {
    for line in text.lines() {
        let line = line.trim_end().replace("**", "");
        let heading = line.trim_start_matches('#');
        if heading.len() < line.len() && heading.starts_with(' ') {
            ui.add_space(2.0);
            ui.label(RichText::new(heading.trim()).strong());
        } else if let Some(item) = line
            .trim_start()
            .strip_prefix("- ")
            .or_else(|| line.trim_start().strip_prefix("* "))
        {
            ui.label(RichText::new(format!("•  {item}")).weak());
        } else if !line.trim().is_empty() {
            ui.label(RichText::new(line.trim()).weak());
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    const PAGE: &str = "https://github.com/Gerrit91/guitarscore/releases/tag/v99.0.0";

    fn answer(tag: &str) -> Result<String, String> {
        Ok(serde_json::json!({
            "tag_name": tag,
            "name": format!("GuitarScore {tag}"),
            "html_url": PAGE,
            "body": "Notes",
        })
        .to_string())
    }

    /// Runs the frame logic without drawing until the fake answer is taken.
    fn checked(app: &mut GuitarScoreApp, manual: bool) {
        let ctx = egui::Context::default();
        app.start_update_check(&ctx, manual);
        app.receive_update_answer();
        assert!(app.updates.running.is_none());
    }

    fn app_answering(fake: Result<String, String>) -> GuitarScoreApp {
        let mut app = GuitarScoreApp::new(&egui::Context::default(), None);
        app.updates.fake_answer = Some(fake);
        app
    }

    #[test]
    fn a_newer_release_is_shown() {
        let mut app = app_answering(answer("v99.0.0"));
        checked(&mut app, false);
        let (release, manual) = app.updates.found.clone().unwrap();
        assert_eq!(release.tag, "v99.0.0");
        assert!(!manual);
    }

    #[test]
    fn a_skipped_release_is_offered_again_only_when_asked_for() {
        let mut app = app_answering(answer("v99.0.0"));
        app.settings.skipped_update = Some("v99.0.0".to_owned());
        checked(&mut app, false);
        assert!(app.updates.found.is_none());
        checked(&mut app, true);
        assert!(app.updates.found.is_some());
    }

    #[test]
    fn an_up_to_date_build_says_so_only_when_asked() {
        let mut app = app_answering(answer("v0.0.1"));
        checked(&mut app, false);
        assert_eq!((app.status.clone(), app.notice.clone()), (None, None));
        checked(&mut app, true);
        assert!(app.status.unwrap().contains("is the latest version"));
        assert!(app.updates.found.is_none());
    }

    #[test]
    fn failures_are_quiet_unless_asked_for() {
        let mut app = app_answering(Err("GitHub could not be reached (offline)".to_owned()));
        checked(&mut app, false);
        assert_eq!(app.notice, None);
        checked(&mut app, true);
        assert_eq!(
            app.notice.as_deref(),
            Some("Could not check for updates: GitHub could not be reached (offline)")
        );
        // A broken answer is an error too, not a panic.
        let mut app = app_answering(Ok("{\"tag_name\": [".to_owned()));
        checked(&mut app, true);
        assert!(
            app.notice
                .unwrap()
                .starts_with("Could not check for updates")
        );
    }

    #[test]
    fn without_permission_nothing_is_asked_or_checked() {
        let ctx = egui::Context::default();
        let mut app = GuitarScoreApp::new(&ctx, None);
        app.updates.fake_answer = Some(answer("v99.0.0"));
        app.show_updates(&ctx);
        assert!(app.updates.running.is_none(), "not enabled: no check");
        assert!(!app.updates.asked);
    }

    #[test]
    fn the_new_release_window_matches_its_snapshot() {
        use egui_kittest::kittest::Queryable as _;

        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 700.0))
            .wgpu()
            .build_eframe(|cc| {
                let mut app = GuitarScoreApp::new(&cc.egui_ctx, None);
                app.updates.current = Current("v0.2.0");
                app.updates.found = Some((
                    Release {
                        tag: "v0.3.0".to_owned(),
                        title: "GuitarScore 0.3.0".to_owned(),
                        url: PAGE.to_owned(),
                        notes: "## What's new\n- Tones for every instrument\n- Faster \
                                opening of large files\n- Fixes for GP5 import\n\n**Full Changelog**: v0.2.0...v0.3.0"
                            .to_owned(),
                    },
                    false,
                ));
                app
            });
        harness.run();
        harness.snapshot("update_available");
        harness.get_by_label("Skip this version").click();
        harness.run();
        let app = harness.state();
        assert!(app.updates.found.is_none());
        assert_eq!(app.settings.skipped_update.as_deref(), Some("v0.3.0"));
    }

    #[test]
    fn the_daily_check_runs_once_when_due() {
        let ctx = egui::Context::default();
        let mut app = app_answering(answer("v99.0.0"));
        app.enable_automatic_update_check();
        app.settings.check_for_updates = Some(true);
        app.settings.update_checked_at = Some(update::now_seconds());
        app.show_updates(&ctx);
        assert!(app.updates.running.is_none(), "checked less than a day ago");

        let mut app = app_answering(answer("v99.0.0"));
        app.enable_automatic_update_check();
        app.settings.check_for_updates = Some(true);
        app.show_updates(&ctx);
        assert!(app.updates.running.is_some(), "never checked: due");
        assert!(app.settings.update_checked_at.is_some());
        app.updates.running = None;
        app.show_updates(&ctx);
        assert!(app.updates.running.is_none(), "only once per run");
    }
}

//! A plugin's own editor (M24 step 4): the plugin opens it as a floating window of its
//! own (X11 on Linux, through XWayland on a Wayland desktop; Win32 and Cocoa elsewhere), so
//! the host draws nothing and needs no window system code. The host keeps the editor alive
//! by running the timers and file descriptors the plugin registers ([`Effect::tick`]),
//! on the plugin's main thread.

use std::ffi::CString;
use std::sync::atomic::Ordering;
use std::time::Instant;

use clack_extensions::gui::{GuiApiType, GuiConfiguration, PluginGui};
use clack_extensions::timer::TimerId;

use crate::{Effect, PluginError};

/// The window system a floating editor uses here.
fn api() -> GuiApiType<'static> {
    if cfg!(windows) {
        GuiApiType::WIN32
    } else if cfg!(target_os = "macos") {
        GuiApiType::COCOA
    } else {
        GuiApiType::X11
    }
}

fn floating() -> GuiConfiguration<'static> {
    GuiConfiguration {
        api_type: api(),
        is_floating: true,
    }
}

impl Effect {
    fn gui(&self) -> Option<PluginGui> {
        self.instance
            .access_shared_handler(|h| h.gui.get().copied().flatten())
    }

    /// Whether the plugin has an editor that can float in a window of its own here.
    pub fn has_editor(&mut self) -> bool {
        let Some(gui) = self.gui() else {
            return false;
        };
        gui.is_api_supported(&self.instance.plugin_handle(), floating())
    }

    /// Opens the plugin's editor in a floating window titled `title`, at `scale` (the
    /// display's scale, 1 for none; a plugin may ignore it).
    pub fn open_editor(&mut self, title: &str, scale: f64) -> Result<(), PluginError> {
        let id = self.id().to_owned();
        let fail = move |reason: &str| PluginError::Unsupported {
            id: id.clone(),
            reason: reason.to_owned(),
        };
        if self.editor_open {
            return Ok(());
        }
        let gui = self.gui().ok_or_else(|| fail("it has no editor"))?;
        if !self.has_editor() {
            return Err(fail("its editor cannot open in a window of its own here"));
        }
        self.instance
            .access_shared_handler(|h| h.editor_closed.store(false, Ordering::Release));
        let handle = self.instance.plugin_handle();
        gui.create(&handle, floating())
            .map_err(|_| fail("its editor could not be created"))?;
        if scale.is_finite() && scale > 0.0 {
            // Some plugins size themselves from the screen instead: not an error.
            let _ = gui.set_scale(&handle, scale);
        }
        if let Ok(title) = CString::new(title.replace('\0', "")) {
            gui.suggest_title(&handle, &title);
        }
        if gui.show(&handle).is_err() {
            gui.destroy(&handle);
            return Err(fail("its editor could not be shown"));
        }
        self.editor_open = true;
        Ok(())
    }

    /// Closes the editor (nothing if it is not open).
    pub fn close_editor(&mut self) {
        if !self.editor_open {
            return;
        }
        if let Some(gui) = self.gui() {
            let handle = self.instance.plugin_handle();
            let _ = gui.hide(&handle);
            gui.destroy(&handle);
        }
        self.editor_open = false;
    }

    /// Whether the editor is open.
    pub fn editor_open(&self) -> bool {
        self.editor_open
    }

    /// Runs the plugin's due timers and lets it read its file descriptors (its editor's
    /// window events). Call it often (every 10 to 20 ms) on the effect's thread. Returns
    /// true once when the user closed the editor's window.
    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        let due: Vec<u32> = self.instance.access_handler(|main| {
            let mut timers = main.timers.borrow_mut();
            timers
                .iter_mut()
                .filter(|timer| timer.due <= now)
                .map(|timer| {
                    timer.due = now + timer.period;
                    timer.id
                })
                .collect()
        });
        let timer = self
            .instance
            .access_shared_handler(|h| h.timer.get().copied().flatten());
        if let Some(timer) = timer {
            let handle = self.instance.plugin_handle();
            for id in due {
                timer.on_timer(&handle, TimerId(id));
            }
        }
        #[cfg(unix)]
        {
            use clack_extensions::posix_fd::FdFlags;
            // Without a poll of our own (std has none), each file descriptor is offered
            // every tick: plugins read their window system's connection without waiting.
            let fds: Vec<i32> = self
                .instance
                .access_handler(|main| main.fds.borrow().clone());
            let posix = self
                .instance
                .access_shared_handler(|h| h.posix_fd.get().copied().flatten());
            if let Some(posix) = posix {
                let handle = self.instance.plugin_handle();
                for fd in fds {
                    posix.on_fd(&handle, fd, FdFlags::READ);
                }
            }
        }
        let closed = self
            .instance
            .access_shared_handler(|h| h.editor_closed.swap(false, Ordering::AcqRel));
        if closed && self.editor_open {
            // The user closed the window: the editor's resources go too.
            if let Some(gui) = self.gui() {
                gui.destroy(&self.instance.plugin_handle());
            }
            self.editor_open = false;
            return true;
        }
        false
    }
}

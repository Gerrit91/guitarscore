//! What GuitarScore answers when a plugin calls the host: requests are noted in flags that
//! the owner of the plugin ([`crate::Effect`]) acts on later, on its own thread. Nothing
//! here blocks, allocates on the audio thread or panics.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use clack_extensions::audio_ports::{
    AudioPortRescanFlags, HostAudioPorts, HostAudioPortsImpl, PluginAudioPorts,
};
use clack_extensions::gui::{GuiSize, HostGui, HostGuiImpl, PluginGui};
use clack_extensions::latency::{HostLatency, HostLatencyImpl, PluginLatency};
use clack_extensions::log::{HostLog, HostLogImpl, LogSeverity};
use clack_extensions::params::{
    HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags, ParamRescanFlags,
    PluginParams,
};
#[cfg(unix)]
use clack_extensions::posix_fd::{FdFlags, HostPosixFd, HostPosixFdImpl, PluginPosixFd};
use clack_extensions::state::{HostState, HostStateImpl, PluginState};
use clack_extensions::timer::{HostTimer, HostTimerImpl, PluginTimer, TimerId};
use clack_host::prelude::*;

/// The plugin's log lines kept (the newest).
const LOG_LINES: usize = 32;

/// The host's type for clack.
pub(crate) struct GsHost;

impl HostHandlers for GsHost {
    type Shared<'a> = Shared;
    type MainThread<'a> = MainThread<'a>;
    type AudioProcessor<'a> = ();

    fn declare_extensions(builder: &mut HostExtensions<Self>, _shared: &Self::Shared<'_>) {
        builder
            .register::<HostLog>()
            .register::<HostParams>()
            .register::<HostState>()
            .register::<HostLatency>()
            .register::<HostAudioPorts>()
            .register::<HostGui>()
            .register::<HostTimer>();
        #[cfg(unix)]
        builder.register::<HostPosixFd>();
    }
}

/// The plugin's extensions found when it was made, and its requests (any thread).
#[derive(Default)]
pub(crate) struct Shared {
    pub(crate) params: OnceLock<Option<PluginParams>>,
    pub(crate) state: OnceLock<Option<PluginState>>,
    pub(crate) latency: OnceLock<Option<PluginLatency>>,
    pub(crate) audio_ports: OnceLock<Option<PluginAudioPorts>>,
    pub(crate) gui: OnceLock<Option<PluginGui>>,
    pub(crate) timer: OnceLock<Option<PluginTimer>>,
    #[cfg(unix)]
    pub(crate) posix_fd: OnceLock<Option<PluginPosixFd>>,
    /// The plugin's editor window was closed (by the user, or its connection lost).
    pub(crate) editor_closed: AtomicBool,
    /// The plugin wants `on_main_thread` called.
    pub(crate) callback: AtomicBool,
    /// The plugin wants to be deactivated and activated again.
    pub(crate) restart: AtomicBool,
    /// The plugin's settings changed (it was edited in its own window).
    pub(crate) dirty: AtomicBool,
    /// The parameters' list or ranges changed.
    pub(crate) rescan: AtomicBool,
    pub(crate) latency_changed: AtomicBool,
    log: Mutex<VecDeque<String>>,
}

impl Shared {
    /// The plugin's log lines so far, oldest first (emptied).
    pub(crate) fn take_log(&self) -> Vec<String> {
        self.log
            .lock()
            .map(|mut log| log.drain(..).collect())
            .unwrap_or_default()
    }
}

impl<'a> SharedHandler<'a> for Shared {
    fn initializing(&self, instance: InitializingPluginHandle<'a>) {
        // Set once, while the plugin is made: a second call cannot happen, and would
        // only be ignored.
        let _ = self.params.set(instance.get_extension());
        let _ = self.state.set(instance.get_extension());
        let _ = self.latency.set(instance.get_extension());
        let _ = self.audio_ports.set(instance.get_extension());
        let _ = self.gui.set(instance.get_extension());
        let _ = self.timer.set(instance.get_extension());
        #[cfg(unix)]
        let _ = self.posix_fd.set(instance.get_extension());
    }

    fn request_restart(&self) {
        self.restart.store(true, Ordering::Release);
    }

    fn request_process(&self) {
        // GuitarScore processes every block anyway.
    }

    fn request_callback(&self) {
        self.callback.store(true, Ordering::Release);
    }
}

impl HostLogImpl for Shared {
    fn log(&self, severity: LogSeverity, message: &str) {
        // May come from the audio thread: never wait for the lock.
        if let Ok(mut log) = self.log.try_lock() {
            if log.len() >= LOG_LINES {
                log.pop_front();
            }
            log.push_back(format!("[{severity}] {message}"));
        }
    }
}

impl HostParamsImplShared for Shared {
    fn request_flush(&self) {
        // Parameter changes are sent with every processed block.
    }
}

impl HostGuiImpl for Shared {
    fn resize_hints_changed(&self) {}

    fn request_resize(&self, _new_size: GuiSize) -> Result<(), HostError> {
        // The editor floats: its window is its own to size.
        Ok(())
    }

    fn request_show(&self) -> Result<(), HostError> {
        Ok(())
    }

    fn request_hide(&self) -> Result<(), HostError> {
        Ok(())
    }

    fn closed(&self, _was_destroyed: bool) {
        self.editor_closed.store(true, Ordering::Release);
    }
}

/// A timer the plugin asked for (its editor's redraws, mostly).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timer {
    pub(crate) id: u32,
    pub(crate) period: Duration,
    pub(crate) due: Instant,
}

/// The shortest timer period run (a plugin asking for less gets this).
const MIN_PERIOD: Duration = Duration::from_millis(10);
/// The most timers and file descriptors a plugin may register.
const MAX_HOOKS: usize = 64;

/// The host's side on the plugin's main thread.
pub(crate) struct MainThread<'a> {
    pub(crate) shared: &'a Shared,
    pub(crate) timers: RefCell<Vec<Timer>>,
    next_timer: Cell<u32>,
    /// File descriptors the plugin's editor wants watched (its X11 connection).
    pub(crate) fds: RefCell<Vec<i32>>,
}

impl<'a> MainThread<'a> {
    pub(crate) fn new(shared: &'a Shared) -> Self {
        Self {
            shared,
            timers: RefCell::new(Vec::new()),
            next_timer: Cell::new(1),
            fds: RefCell::new(Vec::new()),
        }
    }
}

impl HostTimerImpl for MainThread<'_> {
    fn register_timer(&self, period_ms: u32) -> Result<TimerId, HostError> {
        let mut timers = self.timers.borrow_mut();
        if timers.len() >= MAX_HOOKS {
            return Err(HostError::Message("too many timers"));
        }
        let id = self.next_timer.get();
        self.next_timer.set(id.wrapping_add(1).max(1));
        let period = Duration::from_millis(u64::from(period_ms)).max(MIN_PERIOD);
        timers.push(Timer {
            id,
            period,
            due: Instant::now() + period,
        });
        Ok(TimerId(id))
    }

    fn unregister_timer(&self, timer_id: TimerId) -> Result<(), HostError> {
        self.timers
            .borrow_mut()
            .retain(|timer| timer.id != timer_id.0);
        Ok(())
    }
}

#[cfg(unix)]
impl HostPosixFdImpl for MainThread<'_> {
    fn register_fd(&self, fd: i32, _flags: FdFlags) -> Result<(), HostError> {
        let mut fds = self.fds.borrow_mut();
        if fds.len() >= MAX_HOOKS {
            return Err(HostError::Message("too many file descriptors"));
        }
        if !fds.contains(&fd) {
            fds.push(fd);
        }
        Ok(())
    }

    fn modify_fd(&self, _fd: i32, _flags: FdFlags) -> Result<(), HostError> {
        Ok(())
    }

    fn unregister_fd(&self, fd: i32) -> Result<(), HostError> {
        self.fds.borrow_mut().retain(|held| *held != fd);
        Ok(())
    }
}

impl<'a> MainThreadHandler<'a> for MainThread<'a> {}

impl HostParamsImplMainThread for MainThread<'_> {
    fn rescan(&self, _flags: ParamRescanFlags) {
        self.shared.rescan.store(true, Ordering::Release);
    }

    fn clear(&self, _param_id: ClapId, _flags: ParamClearFlags) {
        // GuitarScore keeps no automation or modulation of its own yet.
    }
}

impl HostStateImpl for MainThread<'_> {
    fn mark_dirty(&self) {
        self.shared.dirty.store(true, Ordering::Release);
    }
}

impl HostLatencyImpl for MainThread<'_> {
    fn changed(&self) {
        self.shared.latency_changed.store(true, Ordering::Release);
    }
}

impl HostAudioPortsImpl for MainThread<'_> {
    fn is_rescan_flag_supported(&self, _flag: AudioPortRescanFlags) -> bool {
        // Ports change only through a restart, which GuitarScore does.
        false
    }

    fn rescan(&self, _flag: AudioPortRescanFlags) {
        self.shared.restart.store(true, Ordering::Release);
    }
}

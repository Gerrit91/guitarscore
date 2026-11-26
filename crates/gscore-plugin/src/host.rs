//! What GuitarScore answers when a plugin calls the host: requests are noted in flags that
//! the owner of the plugin ([`crate::Effect`]) acts on later, on its own thread. Nothing
//! here blocks, allocates on the audio thread or panics.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use clack_extensions::audio_ports::{
    AudioPortRescanFlags, HostAudioPorts, HostAudioPortsImpl, PluginAudioPorts,
};
use clack_extensions::latency::{HostLatency, HostLatencyImpl, PluginLatency};
use clack_extensions::log::{HostLog, HostLogImpl, LogSeverity};
use clack_extensions::params::{
    HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags, ParamRescanFlags,
    PluginParams,
};
use clack_extensions::state::{HostState, HostStateImpl, PluginState};
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
            .register::<HostAudioPorts>();
    }
}

/// The plugin's extensions found when it was made, and its requests (any thread).
#[derive(Default)]
pub(crate) struct Shared {
    pub(crate) params: OnceLock<Option<PluginParams>>,
    pub(crate) state: OnceLock<Option<PluginState>>,
    pub(crate) latency: OnceLock<Option<PluginLatency>>,
    pub(crate) audio_ports: OnceLock<Option<PluginAudioPorts>>,
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

/// The host's side on the plugin's main thread.
pub(crate) struct MainThread<'a> {
    pub(crate) shared: &'a Shared,
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

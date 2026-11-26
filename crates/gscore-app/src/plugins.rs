//! The app's plugins (M24, experimental, desktop only): the plugin thread (started when
//! first needed), the controls of the plugin blocks playing in track chains, and what the
//! Sound chain window shows of a plugin (its parameters, the installed plugins). The
//! browser build has a stand-in with the same calls that never offers a plugin.

use std::path::PathBuf;

use crate::chain_settings::PluginBlock;

/// Where a plugin block plays: (track, place in its chain).
pub(crate) type Owner = (usize, usize);

/// Something asked of the plugin thread: not there yet, there, or why not.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Found<T> {
    Waiting,
    Ready(T),
    Failed(String),
}

/// One parameter, as the parameter view shows it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParamView {
    pub(crate) id: u32,
    pub(crate) name: String,
    /// The plugin's group for it ("Filter"), empty for none.
    pub(crate) module: String,
    pub(crate) min: f64,
    pub(crate) max: f64,
    pub(crate) default: f64,
    pub(crate) stepped: bool,
    /// Its value with the block's saved settings loaded.
    pub(crate) value: f64,
}

/// What the parameter view needs of a plugin.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct PluginView {
    /// The parameters a user may turn (hidden and read-only ones left out).
    pub(crate) params: Vec<ParamView>,
    pub(crate) latency: u32,
}

/// One installed plugin a block can hold.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Listed {
    pub(crate) library: PathBuf,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) vendor: String,
    pub(crate) features: Vec<String>,
}

impl Listed {
    /// A fresh block of this plugin: its own defaults, nothing turned.
    pub(crate) fn block(&self) -> PluginBlock {
        PluginBlock {
            library: self.library.clone(),
            id: self.id.clone(),
            name: self.name.clone(),
            vendor: self.vendor.clone(),
            state: String::new(),
            values: Vec::new(),
        }
    }
}

/// The installed plugins found by a scan, and the libraries that could not be opened.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Installed {
    /// The effects (a block holds effects only, for now), sorted by name.
    pub(crate) effects: Vec<Listed>,
    /// Libraries that failed to open, with why.
    pub(crate) problems: Vec<String>,
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::Plugins;
#[cfg(target_arch = "wasm32")]
pub(crate) use web::Plugins;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::mpsc::{Receiver, TryRecvError};

    use gscore_audio::{PluginControl, PluginSource};
    use gscore_guitar::{BlockKind, External};
    use gscore_plugin::{Description, PluginError, PluginThread, Scanned};

    use super::{Found, Installed, Listed, Owner, ParamView, PluginBlock, PluginView};

    /// A request to the plugin thread and its answer once it came.
    #[derive(Debug)]
    enum Asked<T, A> {
        Waiting(Receiver<A>),
        Done(Found<T>),
    }

    /// The plugins of the app.
    #[derive(Debug, Default)]
    pub(crate) struct Plugins {
        enabled: bool,
        thread: Option<PluginThread>,
        /// Why the thread could not be started.
        thread_error: Option<String>,
        controls: HashMap<Owner, Arc<PluginControl>>,
        views: HashMap<String, Asked<PluginView, Result<Description, PluginError>>>,
        installed: Option<Asked<Installed, Vec<Scanned>>>,
    }

    impl Plugins {
        /// Whether plugins may be used (Settings › Audio I/O, experimental).
        pub(crate) fn enabled(&self) -> bool {
            self.enabled
        }

        pub(crate) fn set_enabled(&mut self, enabled: bool) {
            self.enabled = enabled;
        }

        fn handle(&mut self) -> Option<gscore_plugin::PluginHandle> {
            if self.thread.is_none() && self.thread_error.is_none() {
                match PluginThread::spawn() {
                    Ok(thread) => self.thread = Some(thread),
                    Err(error) => self.thread_error = Some(error.to_string()),
                }
            }
            self.thread.as_ref().map(PluginThread::handle)
        }

        /// The engine's block for `block`, or `None` while plugins are off. A block
        /// playing in a track's chain (`owner`) keeps one control, so the parameter view
        /// reaches it while it plays; others (export) get a control of their own.
        pub(crate) fn block_kind(
            &mut self,
            block: &PluginBlock,
            owner: Option<Owner>,
        ) -> Option<BlockKind> {
            if !self.enabled {
                return None;
            }
            let handle = self.handle()?;
            let control = match owner {
                Some(owner) => Arc::clone(self.controls.entry(owner).or_default()),
                None => Arc::new(PluginControl::default()),
            };
            control.set_values(&block.values);
            let source = PluginSource::new(
                handle,
                &block.library,
                &block.id,
                block.state_bytes(),
                control,
            );
            Some(BlockKind::External(External(Arc::new(source))))
        }

        /// Plays the block at `owner` with `values` from now on.
        pub(crate) fn set_values(&self, owner: Owner, values: &[(u32, f64)]) {
            if let Some(control) = self.controls.get(&owner) {
                control.set_values(values);
            }
        }

        /// Why the block at `owner` does not play, if it does not.
        pub(crate) fn problem(&self, owner: Owner) -> Option<String> {
            if let Some(error) = &self.thread_error {
                return Some(error.clone());
            }
            self.controls.get(&owner).and_then(|c| c.problem())
        }

        /// The parameters of `block`'s plugin with its saved settings loaded.
        pub(crate) fn view(&mut self, block: &PluginBlock) -> Found<&PluginView> {
            let key = format!("{}#{}#{}", block.library.display(), block.id, block.state);
            if !self.views.contains_key(&key) {
                let Some(handle) = self.handle() else {
                    return Found::Failed(self.thread_error.clone().unwrap_or_default());
                };
                let answer = handle.describe(&block.library, &block.id, &block.state_bytes());
                self.views.insert(key.clone(), Asked::Waiting(answer));
            }
            let Some(asked) = self.views.get_mut(&key) else {
                return Found::Waiting;
            };
            poll(asked, |answer| match answer {
                Ok(description) => Found::Ready(view_of(description)),
                Err(error) => Found::Failed(error.to_string()),
            });
            match asked {
                Asked::Waiting(_) => Found::Waiting,
                Asked::Done(Found::Ready(view)) => Found::Ready(view),
                Asked::Done(Found::Failed(error)) => Found::Failed(error.clone()),
                Asked::Done(Found::Waiting) => Found::Waiting,
            }
        }

        /// The installed effects: scanned once (in the background) on the first call,
        /// again after [`Self::rescan`].
        pub(crate) fn installed(&mut self) -> Found<&Installed> {
            if self.installed.is_none() {
                let Some(handle) = self.handle() else {
                    return Found::Failed(self.thread_error.clone().unwrap_or_default());
                };
                self.installed = Some(Asked::Waiting(handle.scan(gscore_plugin::default_dirs())));
            }
            let Some(asked) = self.installed.as_mut() else {
                return Found::Waiting;
            };
            poll(asked, |scanned| Found::Ready(installed_of(scanned)));
            match asked {
                Asked::Done(Found::Ready(installed)) => Found::Ready(installed),
                Asked::Done(Found::Failed(error)) => Found::Failed(error.clone()),
                Asked::Waiting(_) | Asked::Done(Found::Waiting) => Found::Waiting,
            }
        }

        /// Looks for installed plugins again on the next [`Self::installed`].
        pub(crate) fn rescan(&mut self) {
            self.installed = None;
        }
    }

    /// Takes the answer of `asked` if it came.
    fn poll<T, A>(asked: &mut Asked<T, A>, answer: impl FnOnce(A) -> Found<T>) {
        if let Asked::Waiting(receiver) = asked {
            match receiver.try_recv() {
                Ok(value) => *asked = Asked::Done(answer(value)),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    *asked = Asked::Done(Found::Failed("the plugin thread stopped".into()));
                }
            }
        }
    }

    fn view_of(description: Description) -> PluginView {
        let params = description
            .params
            .iter()
            .zip(description.values.iter().copied())
            .filter(|(param, _)| !param.hidden && !param.read_only)
            .filter(|(param, _)| param.min.is_finite() && param.max.is_finite())
            .filter(|(param, _)| param.max > param.min)
            .map(|(param, value)| ParamView {
                id: param.id,
                name: param.name.clone(),
                module: param.module.clone(),
                min: param.min,
                max: param.max,
                default: param.default,
                stepped: param.stepped,
                value: if value.is_finite() {
                    value.clamp(param.min, param.max)
                } else {
                    param.default
                },
            })
            .collect();
        PluginView {
            params,
            latency: description.latency,
        }
    }

    fn installed_of(scanned: Vec<Scanned>) -> Installed {
        let mut installed = Installed::default();
        for (library, found) in scanned {
            match found {
                Ok(plugins) => installed.effects.extend(
                    plugins
                        .into_iter()
                        .filter(gscore_plugin::PluginInfo::is_effect)
                        .map(|info| Listed {
                            library: library.clone(),
                            id: info.id,
                            name: info.name,
                            vendor: info.vendor,
                            features: info.features,
                        }),
                ),
                Err(error) => installed.problems.push(error.to_string()),
            }
        }
        installed
            .effects
            .sort_by_cached_key(|listed| (listed.name.to_lowercase(), listed.id.clone()));
        installed
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use gscore_guitar::BlockKind;

    use super::{Found, Installed, Owner, PluginBlock, PluginView};

    /// The browser has no plugins: plugins stay off.
    #[derive(Debug, Default)]
    pub(crate) struct Plugins;

    impl Plugins {
        pub(crate) fn enabled(&self) -> bool {
            false
        }

        pub(crate) fn set_enabled(&mut self, _enabled: bool) {}

        pub(crate) fn block_kind(
            &mut self,
            _block: &PluginBlock,
            _owner: Option<Owner>,
        ) -> Option<BlockKind> {
            None
        }

        pub(crate) fn set_values(&self, _owner: Owner, _values: &[(u32, f64)]) {}

        pub(crate) fn problem(&self, _owner: Owner) -> Option<String> {
            Some("plugins are not available in the browser".into())
        }

        pub(crate) fn view(&mut self, _block: &PluginBlock) -> Found<&PluginView> {
            Found::Failed("plugins are not available in the browser".into())
        }

        pub(crate) fn installed(&mut self) -> Found<&Installed> {
            Found::Failed("plugins are not available in the browser".into())
        }

        pub(crate) fn rescan(&mut self) {}
    }
}

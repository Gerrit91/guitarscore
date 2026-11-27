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
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::mpsc::{self, Receiver, TryRecvError};

    use gscore_audio::{PluginControl, PluginSource};
    use gscore_guitar::{BlockKind, External};
    use gscore_plugin::{Param, PluginError, PluginInfo, remote};

    use super::{Found, Installed, Listed, Owner, ParamView, PluginBlock, PluginView};

    /// What `remote::describe` answers.
    type Described = Result<(PluginInfo, Vec<Param>, Vec<f64>, u32), PluginError>;
    /// One library's plugins, or why it could not be opened.
    type Scanned = Vec<(PathBuf, Result<Vec<PluginInfo>, PluginError>)>;

    /// A request to a plugin host and its answer once it came.
    #[derive(Debug)]
    enum Asked<T, A> {
        Waiting(Receiver<A>),
        Done(Found<T>),
    }

    /// The plugins of the app. Every plugin runs in a host process of its own (also to
    /// be described or listed), so a plugin that crashes never takes the app with it.
    #[derive(Debug, Default)]
    pub(crate) struct Plugins {
        enabled: bool,
        controls: HashMap<Owner, Arc<PluginControl>>,
        views: HashMap<String, Asked<PluginView, Described>>,
        installed: Option<Asked<Installed, Scanned>>,
    }

    /// Runs `work` on a thread of its own; its answer comes on the receiver (an error if
    /// the thread could not start).
    fn background<A: Send + 'static>(
        work: impl FnOnce() -> A + Send + 'static,
        failed: impl FnOnce(String) -> A,
    ) -> Receiver<A> {
        let (reply, answer) = mpsc::channel();
        let sender = reply.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("plugin request".into())
            .spawn(move || {
                let _ = sender.send(work());
            })
        {
            let _ = reply.send(failed(error.to_string()));
        }
        answer
    }

    impl Plugins {
        /// Whether plugins may be used (Settings › Audio I/O, experimental).
        pub(crate) fn enabled(&self) -> bool {
            self.enabled
        }

        pub(crate) fn set_enabled(&mut self, enabled: bool) {
            self.enabled = enabled;
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
            let control = match owner {
                Some(owner) => Arc::clone(self.controls.entry(owner).or_default()),
                None => Arc::new(PluginControl::default()),
            };
            control.set_values(&block.values);
            let source = PluginSource::new(&block.library, &block.id, block.state_bytes(), control);
            Some(BlockKind::External(External(Arc::new(source))))
        }

        /// Plays the block at `owner` with `values` from now on.
        pub(crate) fn set_values(&self, owner: Owner, values: &[(u32, f64)]) {
            if let Some(control) = self.controls.get(&owner) {
                control.set_values(values);
            }
        }

        /// Why the block at `owner` does not play (it could not start, crashed or hung).
        pub(crate) fn problem(&self, owner: Owner) -> Option<String> {
            self.controls.get(&owner).and_then(|c| c.problem())
        }

        /// The parameters of `block`'s plugin with its saved settings loaded.
        pub(crate) fn view(&mut self, block: &PluginBlock) -> Found<&PluginView> {
            let key = format!("{}#{}#{}", block.library.display(), block.id, block.state);
            if !self.views.contains_key(&key) {
                let (library, id, state) =
                    (block.library.clone(), block.id.clone(), block.state_bytes());
                let answer = background(
                    move || remote::describe(&library, &id, &state),
                    |error| Err(PluginError::Process(error)),
                );
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
                Asked::Waiting(_) | Asked::Done(Found::Waiting) => Found::Waiting,
                Asked::Done(Found::Ready(view)) => Found::Ready(view),
                Asked::Done(Found::Failed(error)) => Found::Failed(error.clone()),
            }
        }

        /// The installed effects: scanned once (in the background, each library in a host
        /// of its own) on the first call, again after [`Self::rescan`].
        pub(crate) fn installed(&mut self) -> Found<&Installed> {
            if self.installed.is_none() {
                let answer = background(
                    || {
                        gscore_plugin::bundles_in(&gscore_plugin::default_dirs())
                            .into_iter()
                            .map(|library| {
                                let plugins = remote::scan(&library);
                                (library, plugins)
                            })
                            .collect()
                    },
                    |_| Vec::new(),
                );
                self.installed = Some(Asked::Waiting(answer));
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
                    *asked = Asked::Done(Found::Failed("the plugin request stopped".into()));
                }
            }
        }
    }

    fn view_of(
        (_, params, values, latency): (PluginInfo, Vec<Param>, Vec<f64>, u32),
    ) -> PluginView {
        let params = params
            .iter()
            .zip(values.iter().copied())
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
        PluginView { params, latency }
    }

    fn installed_of(scanned: Scanned) -> Installed {
        let mut installed = Installed::default();
        for (library, found) in scanned {
            match found {
                Ok(plugins) => {
                    installed
                        .effects
                        .extend(
                            plugins
                                .into_iter()
                                .filter(PluginInfo::is_effect)
                                .map(|info| Listed {
                                    library: library.clone(),
                                    id: info.id,
                                    name: info.name,
                                    vendor: info.vendor,
                                    features: info.features,
                                }),
                        )
                }
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

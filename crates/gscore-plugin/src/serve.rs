//! The plugin host process's side (`guitarscore-plugin-host`): reads [`ToHost`] messages
//! from stdin and answers on stdout. One process runs one plugin (or answers one describe
//! or scan request), so a plugin that crashes takes only its own process down.
//!
//! The process's one thread is the plugin's main thread and its audio thread alike: CLAP
//! allows that, and the app's side is what must never wait.

use std::io::{Read, Write};

use crate::wire::{FromHost, ToHost, read_frame, write_frame};
use crate::{Effect, Library, Processor};

/// Answers requests from `input` on `output` until `Quit` or the end of `input`. Returns the
/// process's exit code: 0, or 1 when the app's side sent something unreadable.
pub fn serve(input: &mut impl Read, output: &mut impl Write) -> i32 {
    let mut running: Option<(Effect, Processor)> = None;
    loop {
        let frame = match read_frame(input) {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(_) => return 1,
        };
        let Ok(message) = ToHost::decode(&frame) else {
            return 1;
        };
        let answer = match message {
            ToHost::Quit => break,
            ToHost::Start {
                library,
                id,
                state,
                config,
            } => {
                let started = (|| {
                    let library = Library::open(&library)?;
                    let mut effect = Effect::new(&library, &id)?;
                    if !state.is_empty() {
                        effect.load_state(&state)?;
                    }
                    let latency = effect.latency();
                    let processor = effect.activate(config)?;
                    Ok::<_, crate::PluginError>((effect, processor, latency))
                })();
                Some(FromHost::Started(match started {
                    Ok((effect, processor, latency)) => {
                        running = Some((effect, processor));
                        Ok(latency)
                    }
                    Err(error) => Err(error.to_string()),
                }))
            }
            ToHost::Process {
                params,
                mut left,
                mut right,
            } => Some(FromHost::Processed(match &mut running {
                Some((effect, processor)) => {
                    for (id, value) in params {
                        processor.set_param(id, value);
                    }
                    let result = processor.process(&mut left, &mut right);
                    // The plugin's main-thread requests, between blocks.
                    let _ = effect.idle();
                    result.map(|()| (left, right)).map_err(|e| e.to_string())
                }
                None => Err("no plugin is running".into()),
            })),
            ToHost::Reset => {
                if let Some((_, processor)) = &mut running {
                    processor.reset();
                }
                None
            }
            ToHost::Describe { library, id, state } => {
                let described = (|| {
                    let library = Library::open(&library)?;
                    let info = library
                        .plugins()
                        .into_iter()
                        .find(|info| info.id == id)
                        .ok_or_else(|| crate::PluginError::NotFound(id.clone()))?;
                    let mut effect = Effect::new(&library, &id)?;
                    if !state.is_empty() {
                        effect.load_state(&state)?;
                    }
                    let params = effect.params();
                    let values = params
                        .iter()
                        .map(|param| effect.value(param.id).unwrap_or(param.default))
                        .collect();
                    Ok::<_, crate::PluginError>((info, params, values, effect.latency()))
                })();
                Some(FromHost::Described(described.map_err(|e| e.to_string())))
            }
            ToHost::Scan { library } => Some(FromHost::Scanned(
                Library::open(&library)
                    .map(|library| library.plugins())
                    .map_err(|e| e.to_string()),
            )),
        };
        if let Some(answer) = answer
            && write_frame(output, &answer.encode()).is_err()
        {
            // The app is gone.
            break;
        }
    }
    if let Some((mut effect, processor)) = running {
        effect.deactivate(processor);
    }
    0
}

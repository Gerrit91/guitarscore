//! Choosing the audio output device: the devices this machine offers, and opening the one
//! the user chose (by cpal's [`cpal::DeviceId`], kept as text in the settings), with the
//! system's default output as the fallback when it is gone.
//!
//! On Linux the list comes from the PulseAudio host (PulseAudio, or PipeWire's
//! PulseAudio server): the real outputs, with their own names. Without such a server it
//! comes from ALSA, without its plugins and duplicate card entries. The default output is
//! ALSA's either way, as it was before the choice existed: cpal would otherwise switch its
//! default to PulseAudio whenever the feature is built in.

use cpal::traits::{DeviceTrait, HostTrait};

use gscore_play::PlayError;

/// An audio output the user can choose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDevice {
    /// cpal's id as text (`<host>:<device>`): stable across restarts, and what the
    /// settings keep.
    pub id: String,
    /// The name to show.
    pub name: String,
}

/// The audio outputs of this machine, in the host's order. Asking a host can take a while
/// (ALSA probes every card), so call it off the UI thread. A device without an id cannot
/// be chosen again later and is left out; a failing host lists none.
pub fn output_devices() -> Vec<OutputDevice> {
    #[cfg(target_os = "linux")]
    {
        let pulse = cpal::host_from_id(cpal::HostId::PulseAudio)
            .map(|host| list(&host))
            .unwrap_or_default();
        if !pulse.is_empty() {
            return pulse;
        }
        list(&default_host())
            .into_iter()
            .filter(|device| useful_alsa(&device.id))
            .map(|mut device| {
                // ALSA names a card's main output "<card>, " (an empty second part).
                device.name = device.name.trim_end_matches([',', ' ']).to_owned();
                device
            })
            .collect()
    }
    #[cfg(not(target_os = "linux"))]
    {
        list(&default_host())
    }
}

fn list(host: &cpal::Host) -> Vec<OutputDevice> {
    let Ok(devices) = host.output_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| {
            let id = device.id().ok()?.to_string();
            Some(OutputDevice {
                name: name_of(&device, &id),
                id,
            })
        })
        .collect()
}

/// Whether an ALSA entry (`alsa:<pcm>`) is worth offering: a card's own outputs
/// (`sysdefault`, `front`, `hdmi`, `iec958`, a USB stream) and the sound servers'
/// devices, not the resampler, mixing and null plugins, nor the raw `hw`/`plughw` names
/// of outputs already listed.
#[cfg(target_os = "linux")]
fn useful_alsa(id: &str) -> bool {
    let pcm = id.strip_prefix("alsa:").unwrap_or(id);
    let kind = pcm.split(':').next().unwrap_or(pcm);
    matches!(
        kind,
        "default"
            | "sysdefault"
            | "front"
            | "hdmi"
            | "iec958"
            | "usbstream"
            | "pipewire"
            | "pulse"
            | "jack"
    )
}

/// The host the default output belongs to: ALSA on Linux (see the module notes), the
/// platform's own elsewhere.
fn default_host() -> cpal::Host {
    #[cfg(target_os = "linux")]
    if let Ok(host) = cpal::host_from_id(cpal::HostId::Alsa) {
        return host;
    }
    cpal::default_host()
}

/// The device to play through: the one with `id`, or the default output (`None`, or a
/// device that is not there now; the second value then says so).
pub(crate) fn open_output(id: Option<&str>) -> Result<(cpal::Device, Option<String>), PlayError> {
    let mut notice = None;
    if let Some(id) = id {
        let device = id.parse::<cpal::DeviceId>().ok().and_then(|id| {
            let host = cpal::host_from_id(id.host()).ok()?;
            host.device_by_id(&id)
        });
        match device {
            Some(device) => return Ok((device, None)),
            None => {
                notice = Some(
                    "The chosen audio output is not available; playing through the default \
                     output"
                        .to_owned(),
                );
            }
        }
    }
    let device = default_host()
        .default_output_device()
        .ok_or_else(|| PlayError::Device("no audio output device found".to_owned()))?;
    Ok((device, notice))
}

/// `device`'s name to show, or `fallback` if it has none.
pub(crate) fn name_of(device: &cpal::Device, fallback: &str) -> String {
    device
        .description()
        .map(|d| d.name().to_owned())
        .ok()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_the_outputs_never_panics_and_gives_ids_that_parse() {
        // A CI machine may have no sound card: an empty list is fine, a panic is not.
        for device in output_devices() {
            assert!(device.id.parse::<cpal::DeviceId>().is_ok(), "{device:?}");
            assert!(!device.name.is_empty(), "{device:?}");
        }
    }

    #[test]
    fn an_unknown_or_broken_id_falls_back_to_the_default_with_a_notice() {
        for id in [
            "",
            "not an id",
            "alsa:no-such-card-gscore",
            "pulseaudio:no-such-sink",
        ] {
            // Without any output device the fallback itself is an error, which is fine.
            if let Ok((_, notice)) = open_output(Some(id)) {
                assert!(notice.is_some(), "{id:?} gave no notice");
            }
        }
        if let Ok((_, notice)) = open_output(None) {
            assert!(notice.is_none());
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn alsa_plugins_and_raw_card_names_are_not_offered() {
        for id in [
            "alsa:null",
            "alsa:lavrate",
            "alsa:samplerate",
            "alsa:speexrate",
            "alsa:oss",
            "alsa:upmix",
            "alsa:vdownmix",
            "alsa:hw:CARD=0,DEV=0",
            "alsa:plughw:CARD=0,DEV=0",
        ] {
            assert!(!useful_alsa(id), "{id}");
        }
        for id in [
            "alsa:default",
            "alsa:sysdefault:CARD=sofsoundwire",
            "alsa:hdmi:CARD=sofsoundwire,DEV=0",
            "alsa:front:CARD=Generic,DEV=0",
            "alsa:pipewire",
        ] {
            assert!(useful_alsa(id), "{id}");
        }
    }
}

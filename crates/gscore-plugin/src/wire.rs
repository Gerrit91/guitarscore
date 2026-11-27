//! The messages between the app and a plugin host process (M24 step 3), as bytes on its
//! stdin and stdout: each a length (u32, little-endian) and that many bytes, the first of
//! them saying which message it is.
//!
//! What a host sends back is read as hostile: a plugin with a memory fault can write
//! anything. Every length is capped, every read checked; a message that does not decode
//! is an error, and the host is then stopped.

use std::io::{self, Read, Write};
use std::path::PathBuf;

use crate::{AudioConfig, Param, PluginInfo};

/// The largest message read (a scan of a big library, a large settings blob): 64 MiB.
pub(crate) const MAX_MESSAGE: usize = 64 << 20;
/// The most samples a block carries per side.
const MAX_FRAMES: usize = 1 << 16;
/// The most items of a list (plugins, parameters, values).
const MAX_ITEMS: usize = 1 << 16;

/// From the app to the host.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ToHost {
    /// Make plugin `id` of `library` an effect with the settings `state` and start it.
    Start {
        library: PathBuf,
        id: String,
        state: Vec<u8>,
        config: AudioConfig,
    },
    /// One block of sound to run through it, with parameter changes before it.
    Process {
        params: Vec<(u32, f64)>,
        left: Vec<f32>,
        right: Vec<f32>,
    },
    /// Forget the sound so far.
    Reset,
    /// The plugin's parameters with `state` loaded (in a host of its own).
    Describe {
        library: PathBuf,
        id: String,
        state: Vec<u8>,
    },
    /// The plugins of one library (in a host of its own: a library that crashes while it
    /// is opened takes only that host).
    Scan { library: PathBuf },
    /// Stop the plugin and end.
    Quit,
}

/// From the host to the app.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FromHost {
    /// The plugin started (its latency), or why not.
    Started(Result<u32, String>),
    /// A block back (same length as sent), or why the plugin failed on it.
    Processed(Result<(Vec<f32>, Vec<f32>), String>),
    /// The answer to `Describe`: the plugin, its parameters and their values, its latency.
    Described(Result<(PluginInfo, Vec<Param>, Vec<f64>, u32), String>),
    /// The answer to `Scan`.
    Scanned(Result<Vec<PluginInfo>, String>),
}

/// Writes `message` as one frame.
pub(crate) fn write_frame(out: &mut impl Write, body: &[u8]) -> io::Result<()> {
    let len = u32::try_from(body.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "message too long"))?;
    out.write_all(&len.to_le_bytes())?;
    out.write_all(body)?;
    out.flush()
}

/// Reads one frame (`None` at the end of the stream).
pub(crate) fn read_frame(input: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut len = [0_u8; 4];
    match input.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = usize::try_from(u32::from_le_bytes(len)).unwrap_or(usize::MAX);
    if len > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too long",
        ));
    }
    let mut body = vec![0_u8; len];
    input.read_exact(&mut body)?;
    Ok(Some(body))
}

impl ToHost {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        match self {
            ToHost::Start {
                library,
                id,
                state,
                config,
            } => {
                w.u8(0);
                w.path(library);
                w.text(id);
                w.bytes(state);
                w.u32(config.sample_rate);
                w.u32(config.max_frames);
            }
            ToHost::Process {
                params,
                left,
                right,
            } => {
                w.u8(1);
                w.u32(len32(params.len()));
                for &(id, value) in params {
                    w.u32(id);
                    w.f64(value);
                }
                w.samples(left);
                w.samples(right);
            }
            ToHost::Reset => w.u8(2),
            ToHost::Describe { library, id, state } => {
                w.u8(3);
                w.path(library);
                w.text(id);
                w.bytes(state);
            }
            ToHost::Scan { library } => {
                w.u8(4);
                w.path(library);
            }
            ToHost::Quit => w.u8(5),
        }
        w.0
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut r = Reader(bytes);
        let message = match r.u8()? {
            0 => ToHost::Start {
                library: r.path()?,
                id: r.text()?,
                state: r.bytes()?,
                config: AudioConfig {
                    sample_rate: r.u32()?,
                    max_frames: r.u32()?,
                },
            },
            1 => {
                let count = r.count()?;
                let mut params = Vec::with_capacity(count.min(1024));
                for _ in 0..count {
                    params.push((r.u32()?, r.f64()?));
                }
                ToHost::Process {
                    params,
                    left: r.samples()?,
                    right: r.samples()?,
                }
            }
            2 => ToHost::Reset,
            3 => ToHost::Describe {
                library: r.path()?,
                id: r.text()?,
                state: r.bytes()?,
            },
            4 => ToHost::Scan { library: r.path()? },
            5 => ToHost::Quit,
            other => return Err(format!("unknown message {other}")),
        };
        r.end()?;
        Ok(message)
    }
}

impl FromHost {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        match self {
            FromHost::Started(result) => {
                w.u8(0);
                w.result(result, |w, latency| w.u32(*latency));
            }
            FromHost::Processed(result) => {
                w.u8(1);
                w.result(result, |w, (left, right)| {
                    w.samples(left);
                    w.samples(right);
                });
            }
            FromHost::Described(result) => {
                w.u8(2);
                w.result(result, |w, (info, params, values, latency)| {
                    w.info(info);
                    w.u32(len32(params.len()));
                    for param in params {
                        w.param(param);
                    }
                    w.u32(len32(values.len()));
                    for value in values {
                        w.f64(*value);
                    }
                    w.u32(*latency);
                });
            }
            FromHost::Scanned(result) => {
                w.u8(3);
                w.result(result, |w, plugins| {
                    w.u32(len32(plugins.len()));
                    for info in plugins {
                        w.info(info);
                    }
                });
            }
        }
        w.0
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut r = Reader(bytes);
        let message = match r.u8()? {
            0 => FromHost::Started(r.result(Reader::u32)?),
            1 => FromHost::Processed(r.result(|r| Ok((r.samples()?, r.samples()?)))?),
            2 => FromHost::Described(r.result(|r| {
                let info = r.info()?;
                let count = r.count()?;
                let mut params = Vec::with_capacity(count.min(1024));
                for _ in 0..count {
                    params.push(r.param()?);
                }
                let count = r.count()?;
                let mut values = Vec::with_capacity(count.min(1024));
                for _ in 0..count {
                    values.push(r.f64()?);
                }
                Ok((info, params, values, r.u32()?))
            })?),
            3 => FromHost::Scanned(r.result(|r| {
                let count = r.count()?;
                let mut plugins = Vec::with_capacity(count.min(1024));
                for _ in 0..count {
                    plugins.push(r.info()?);
                }
                Ok(plugins)
            })?),
            other => return Err(format!("unknown answer {other}")),
        };
        r.end()?;
        Ok(message)
    }
}

/// A count as written (lists never come near 4 billion items; a longer one is cut).
fn len32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn f64(&mut self, value: f64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.u32(len32(bytes.len()));
        self.0.extend_from_slice(bytes);
    }

    fn text(&mut self, text: &str) {
        self.bytes(text.as_bytes());
    }

    fn path(&mut self, path: &std::path::Path) {
        self.bytes(path.as_os_str().as_encoded_bytes());
    }

    fn samples(&mut self, samples: &[f32]) {
        self.u32(len32(samples.len()));
        for sample in samples {
            self.0.extend_from_slice(&sample.to_le_bytes());
        }
    }

    fn result<T>(&mut self, result: &Result<T, String>, ok: impl FnOnce(&mut Self, &T)) {
        match result {
            Ok(value) => {
                self.u8(1);
                ok(self, value);
            }
            Err(error) => {
                self.u8(0);
                self.text(error);
            }
        }
    }

    fn info(&mut self, info: &PluginInfo) {
        self.text(&info.id);
        self.text(&info.name);
        self.text(&info.vendor);
        self.text(&info.version);
        self.u32(len32(info.features.len()));
        for feature in &info.features {
            self.text(feature);
        }
    }

    fn param(&mut self, param: &Param) {
        self.u32(param.id);
        self.text(&param.name);
        self.text(&param.module);
        self.f64(param.min);
        self.f64(param.max);
        self.f64(param.default);
        self.u8(u8::from(param.stepped)
            | u8::from(param.hidden) << 1
            | u8::from(param.read_only) << 2);
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, len: usize) -> Result<&[u8], String> {
        if self.0.len() < len {
            return Err("the message ends early".into());
        }
        let (head, rest) = self.0.split_at(len);
        self.0 = rest;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, String> {
        self.take(1)?
            .first()
            .copied()
            .ok_or_else(|| "the message ends early".to_owned())
    }

    fn u32(&mut self) -> Result<u32, String> {
        let bytes: [u8; 4] = self.take(4)?.try_into().map_err(|_| "bad number")?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn f64(&mut self) -> Result<f64, String> {
        let bytes: [u8; 8] = self.take(8)?.try_into().map_err(|_| "bad number")?;
        Ok(f64::from_le_bytes(bytes))
    }

    /// A list's length, refused past [`MAX_ITEMS`].
    fn count(&mut self) -> Result<usize, String> {
        let count = usize::try_from(self.u32()?).unwrap_or(usize::MAX);
        if count > MAX_ITEMS {
            return Err("a list is too long".into());
        }
        Ok(count)
    }

    fn bytes(&mut self) -> Result<Vec<u8>, String> {
        let len = usize::try_from(self.u32()?).unwrap_or(usize::MAX);
        Ok(self.take(len)?.to_vec())
    }

    fn text(&mut self) -> Result<String, String> {
        String::from_utf8(self.bytes()?).map_err(|_| "a text is not UTF-8".to_owned())
    }

    fn path(&mut self) -> Result<PathBuf, String> {
        // Paths travel as the platform's bytes; on Unix any bytes are a path, elsewhere
        // they must be UTF-8 (as std accepts them).
        let bytes = self.bytes()?;
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
        }
        #[cfg(not(unix))]
        {
            String::from_utf8(bytes)
                .map(PathBuf::from)
                .map_err(|_| "a path is not UTF-8".to_owned())
        }
    }

    fn samples(&mut self) -> Result<Vec<f32>, String> {
        let len = usize::try_from(self.u32()?).unwrap_or(usize::MAX);
        if len > MAX_FRAMES {
            return Err("a block is too long".into());
        }
        let bytes = self.take(len.saturating_mul(4))?;
        Ok(bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| {
                let sample = f32::from_le_bytes(*chunk);
                if sample.is_finite() { sample } else { 0.0 }
            })
            .collect())
    }

    fn result<T>(
        &mut self,
        ok: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<Result<T, String>, String> {
        match self.u8()? {
            1 => Ok(Ok(ok(self)?)),
            0 => Ok(Err(self.text()?)),
            other => Err(format!("bad result tag {other}")),
        }
    }

    fn info(&mut self) -> Result<PluginInfo, String> {
        let id = self.text()?;
        let name = self.text()?;
        let vendor = self.text()?;
        let version = self.text()?;
        let count = self.count()?;
        let mut features = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            features.push(self.text()?);
        }
        Ok(PluginInfo {
            id,
            name,
            vendor,
            version,
            features,
        })
    }

    fn param(&mut self) -> Result<Param, String> {
        let id = self.u32()?;
        let name = self.text()?;
        let module = self.text()?;
        let (min, max, default) = (self.f64()?, self.f64()?, self.f64()?);
        let flags = self.u8()?;
        Ok(Param {
            id,
            name,
            module,
            min,
            max,
            default,
            stepped: flags & 1 != 0,
            hidden: flags & 2 != 0,
            read_only: flags & 4 != 0,
        })
    }

    fn end(&self) -> Result<(), String> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err("the message has bytes left over".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_reads_back_as_written() {
        let info = PluginInfo {
            id: "a.b".into(),
            name: "Näme".into(),
            vendor: "V".into(),
            version: "1".into(),
            features: vec!["audio-effect".into(), "stereo".into()],
        };
        let param = Param {
            id: 7,
            name: "Gain".into(),
            module: "Out".into(),
            min: -1.0,
            max: 2.0,
            default: 0.5,
            stepped: true,
            hidden: false,
            read_only: true,
        };
        let to_host = [
            ToHost::Start {
                library: "/x/y.clap".into(),
                id: "a.b".into(),
                state: vec![1, 2, 3],
                config: AudioConfig {
                    sample_rate: 48_000,
                    max_frames: 64,
                },
            },
            ToHost::Process {
                params: vec![(1, 0.25)],
                left: vec![0.5, -0.5],
                right: vec![0.0, 1.0],
            },
            ToHost::Reset,
            ToHost::Describe {
                library: "/z.clap".into(),
                id: "c".into(),
                state: Vec::new(),
            },
            ToHost::Scan {
                library: "/z.clap".into(),
            },
            ToHost::Quit,
        ];
        for message in to_host {
            assert_eq!(ToHost::decode(&message.encode()).unwrap(), message);
        }
        let from_host = [
            FromHost::Started(Ok(64)),
            FromHost::Started(Err("no".into())),
            FromHost::Processed(Ok((vec![0.1], vec![0.2]))),
            FromHost::Described(Ok((info.clone(), vec![param], vec![0.5], 3))),
            FromHost::Scanned(Ok(vec![info])),
            FromHost::Scanned(Err("crash".into())),
        ];
        for message in from_host {
            assert_eq!(FromHost::decode(&message.encode()).unwrap(), message);
        }
    }

    #[test]
    fn hostile_bytes_are_errors_never_panics() {
        // Every prefix of a real message, and junk, and lengths that lie.
        let message = FromHost::Described(Ok((
            PluginInfo {
                id: "a".into(),
                name: "b".into(),
                vendor: String::new(),
                version: String::new(),
                features: vec!["x".into()],
            },
            Vec::new(),
            vec![1.0],
            0,
        )))
        .encode();
        for len in 0..message.len() {
            assert!(FromHost::decode(&message[..len]).is_err());
        }
        let mut seed = 0x2545_f491_u32;
        for _ in 0..2000 {
            let bytes: Vec<u8> = (0..64)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    seed as u8
                })
                .collect();
            let _ = FromHost::decode(&bytes);
            let _ = ToHost::decode(&bytes);
        }
        assert!(FromHost::decode(&[1, 1, 255, 255, 255, 255]).is_err());
        // Samples that are not numbers arrive as silence.
        let nan = FromHost::Processed(Ok((vec![f32::NAN], vec![f32::INFINITY]))).encode();
        assert_eq!(
            FromHost::decode(&nan).unwrap(),
            FromHost::Processed(Ok((vec![0.0], vec![0.0])))
        );
    }

    #[test]
    fn frames_are_read_back_and_a_lying_length_is_refused() {
        let mut stream = Vec::new();
        write_frame(&mut stream, b"abc").unwrap();
        write_frame(&mut stream, b"").unwrap();
        let mut input = stream.as_slice();
        assert_eq!(read_frame(&mut input).unwrap(), Some(b"abc".to_vec()));
        assert_eq!(read_frame(&mut input).unwrap(), Some(Vec::new()));
        assert_eq!(read_frame(&mut input).unwrap(), None);
        let huge = u32::MAX.to_le_bytes();
        assert!(read_frame(&mut huge.as_slice()).is_err());
    }
}

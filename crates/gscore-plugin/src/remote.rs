//! Plugins in host processes (M24 step 3): the app's side of `guitarscore-plugin-host`.
//!
//! [`RemoteEffect::start`] starts a host for one plugin and waits (with a time limit) until
//! it runs. The audio thread then hands it sound through [`RemoteEffect::process`], which
//! never waits: blocks of [`BLOCK`] frames go to a writer thread that sends them to the
//! host, a reader thread puts what comes back on a queue, and the audio thread takes each
//! block [`PIPELINE`] blocks after it sent it. That fixed delay is added to the plugin's
//! latency, which the mixer makes up. A block that is not back in time is silence, and is
//! dropped when it comes, so the delay stays fixed.
//!
//! When the host dies, hangs (no answer for [`WATCHDOG`] while blocks wait) or answers
//! garbage, it is stopped and the effect is bypassed: the sound passes unchanged, and
//! [`RemoteEffect::failure`] says why. Editing and playback go on.
//!
//! [`describe`] and [`scan`] run a short-lived host of their own each.

use std::collections::VecDeque;
use std::io::{BufReader, BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::wire::{FromHost, ToHost, read_frame, write_frame};
use crate::{AudioConfig, Param, PluginError, PluginInfo};

/// Frames in one block sent to a host.
pub const BLOCK: usize = 64;
/// Blocks in flight: a block's sound comes back this many blocks after it was sent.
pub const PIPELINE: usize = 8;
/// The latency the host process adds, in frames.
pub const PROCESS_LATENCY: u32 = (BLOCK * PIPELINE) as u32;
/// A host that does not answer for this long while blocks wait is stopped.
pub const WATCHDOG: Duration = Duration::from_secs(2);
/// How long starting a plugin, describing it or scanning a library may take.
pub const START_WAIT: Duration = Duration::from_secs(20);
/// Parameter changes carried with one block.
const MAX_PARAMS: usize = 256;
/// The host program's name.
const HOST: &str = "guitarscore-plugin-host";
/// Started with this argument, the GuitarScore app serves as a plugin host itself (see
/// [`crate::serve`]), so a release needs no program of its own for it.
pub const HOST_FLAG: &str = "--plugin-host";

/// The dedicated host program, if there is one: `GSCORE_PLUGIN_HOST` if set, else next to
/// the running program (or a few folders up, where `cargo test` runs from).
pub fn host_program() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("GSCORE_PLUGIN_HOST") {
        return Some(PathBuf::from(path));
    }
    let name = format!("{HOST}{}", std::env::consts::EXE_SUFFIX);
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .skip(1)
        .take(5)
        .map(|dir| dir.join(&name))
        .find(|path| path.is_file())
}

/// How a host is started: the dedicated program, or else the running program (the app)
/// with [`HOST_FLAG`].
fn host_command() -> Result<Command, String> {
    if let Some(program) = host_program() {
        return Ok(Command::new(program));
    }
    let exe = std::env::current_exe()
        .map_err(|e| format!("cannot find the program to host plugins: {e}"))?;
    let mut command = Command::new(exe);
    command.arg(HOST_FLAG);
    Ok(command)
}

/// One block on its way: sound and the parameter changes to apply before it.
#[derive(Debug)]
struct Packet {
    seq: u64,
    left: Vec<f32>,
    right: Vec<f32>,
    params: Vec<(u32, f64)>,
    /// The plugin forgets its sound before this block.
    reset: bool,
}

impl Packet {
    fn empty() -> Self {
        Self {
            seq: 0,
            left: Vec::with_capacity(BLOCK),
            right: Vec::with_capacity(BLOCK),
            params: Vec::with_capacity(MAX_PARAMS),
            reset: false,
        }
    }
}

/// What the threads and the audio side share.
#[derive(Debug, Default)]
struct Health {
    failed: AtomicBool,
    why: Mutex<Option<String>>,
    /// Blocks sent and blocks answered, for the watchdog.
    sent: AtomicU64,
    answered: AtomicU64,
    /// The effect is being dropped: the watchdog ends.
    closing: AtomicBool,
}

impl Health {
    fn fail(&self, why: String) {
        let mut held = self.why.lock().unwrap_or_else(PoisonError::into_inner);
        if held.is_none() {
            *held = Some(why);
        }
        self.failed.store(true, Ordering::Release);
    }
}

/// How a host is doing, readable from any thread (see [`RemoteEffect::status`]).
#[derive(Debug, Clone)]
pub struct Status(Arc<Health>);

impl Status {
    /// Why the plugin stopped playing, if it did.
    pub fn failure(&self) -> Option<String> {
        if !self.0.failed.load(Ordering::Acquire) {
            return None;
        }
        Some(
            self.0
                .why
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
                .unwrap_or_else(|| "the plugin stopped".into()),
        )
    }
}

/// A plugin running in a host process: the audio thread's side.
#[derive(Debug)]
pub struct RemoteEffect {
    /// Blocks to send, and empty packets to fill.
    outgoing: SyncSender<Packet>,
    free: Receiver<Packet>,
    /// Blocks back from the host, and packets returned for reuse.
    incoming: Receiver<Packet>,
    recycle: SyncSender<Packet>,
    health: Arc<Health>,
    child: Arc<Mutex<Child>>,
    threads: Vec<JoinHandle<()>>,
    latency: u32,
    /// The block being gathered.
    gather: Packet,
    next_seq: u64,
    /// The block being played out, where in it, and the sequence number expected next.
    playing: Option<Packet>,
    played: usize,
    expected: u64,
    /// Silence still to play before the first block is due.
    lead: usize,
    /// Parameter changes for the next block.
    pending: Vec<(u32, f64)>,
    /// The next block sent asks the plugin to forget its sound first.
    reset: bool,
    /// A block that came back before its turn (one before it was lost).
    early: Option<Packet>,
    /// Rendering offline: each block is waited for instead of played as silence.
    offline: bool,
}

impl RemoteEffect {
    /// Starts a host for plugin `id` of `library` with the settings `state` and waits for
    /// it to run (at most [`START_WAIT`]).
    pub fn start(
        library: &Path,
        id: &str,
        state: &[u8],
        config: AudioConfig,
    ) -> Result<Self, PluginError> {
        let config = AudioConfig {
            sample_rate: config.sample_rate,
            max_frames: BLOCK as u32,
        };
        let fail = |reason: String| PluginError::Start {
            id: id.to_owned(),
            reason,
        };
        let (mut child, mut stdin, stdout) = spawn().map_err(fail)?;
        let start = ToHost::Start {
            library: library.to_owned(),
            id: id.to_owned(),
            state: state.to_owned(),
            config,
        };
        // The first answer comes on a thread of its own, so it can be waited for with a
        // time limit.
        let answer = request(&mut stdin, stdout, &start);
        let (latency, stdout) = match answer.recv_timeout(START_WAIT) {
            Ok((Ok(FromHost::Started(Ok(latency))), stdout)) => (latency, stdout),
            Ok((Ok(FromHost::Started(Err(why))), _)) => {
                stop(&mut child);
                return Err(fail(why));
            }
            Ok((Ok(_), _)) | Ok((Err(_), _)) => {
                stop(&mut child);
                return Err(fail("the plugin host answered nonsense".into()));
            }
            Err(_) => {
                stop(&mut child);
                return Err(fail(died_or_late(&mut child)));
            }
        };
        let health = Arc::new(Health::default());
        let child = Arc::new(Mutex::new(child));
        let (outgoing, to_writer) = mpsc::sync_channel::<Packet>(PIPELINE * 2);
        let (recycle, free_pool) = mpsc::sync_channel::<Packet>(PIPELINE * 4);
        let (to_audio, incoming) = mpsc::sync_channel::<Packet>(PIPELINE * 2);
        let (give_free, free) = mpsc::sync_channel::<Packet>(PIPELINE * 4);
        let (sent_seqs, seqs) = mpsc::channel::<u64>();
        for _ in 0..PIPELINE * 3 {
            let _ = give_free.try_send(Packet::empty());
        }
        // The writer: blocks to the host, and the watchdog.
        let writer = {
            let health = Arc::clone(&health);
            let child = Arc::clone(&child);
            std::thread::Builder::new()
                .name("plugin host writer".into())
                .spawn(move || write_blocks(stdin, &to_writer, &sent_seqs, &health, &child))
                .map_err(|e| fail(e.to_string()))?
        };
        // The reader: answers from the host to the audio side; used packets back to free.
        let reader = {
            let health = Arc::clone(&health);
            std::thread::Builder::new()
                .name("plugin host reader".into())
                .spawn(move || {
                    read_blocks(stdout, &seqs, &free_pool, &give_free, &to_audio, &health);
                })
                .map_err(|e| fail(e.to_string()))?
        };
        let watchdog = {
            let health = Arc::clone(&health);
            let child = Arc::clone(&child);
            std::thread::Builder::new()
                .name("plugin host watchdog".into())
                .spawn(move || watch(&health, &child))
                .map_err(|e| fail(e.to_string()))?
        };
        Ok(Self {
            outgoing,
            free,
            incoming,
            recycle,
            health,
            child,
            threads: vec![writer, reader, watchdog],
            latency: latency.saturating_add(PROCESS_LATENCY),
            gather: Packet::empty(),
            next_seq: 0,
            playing: None,
            played: 0,
            expected: 0,
            lead: BLOCK * PIPELINE,
            pending: Vec::with_capacity(MAX_PARAMS),
            reset: false,
            early: None,
            offline: false,
        })
    }

    /// The frames its sound comes back late: the plugin's own latency and the pipeline's.
    pub fn latency(&self) -> u32 {
        self.latency
    }

    /// Why the plugin stopped playing (it crashed, hung or failed), if it did.
    pub fn failure(&self) -> Option<String> {
        self.status().failure()
    }

    /// Whether the plugin stopped playing (cheap; for the audio thread).
    pub fn failed(&self) -> bool {
        self.health.failed.load(Ordering::Acquire)
    }

    /// A handle to how the host is doing, to be read on another thread.
    pub fn status(&self) -> Status {
        Status(Arc::clone(&self.health))
    }

    /// Rendering offline (an export, faster than real time): [`Self::process`] waits for
    /// each block (up to [`WATCHDOG`]) instead of playing silence for one not back yet.
    /// Not for the audio thread.
    pub fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
    }

    /// Sets parameter `id` to `value` from the next block sent on.
    pub fn set_param(&mut self, id: u32, value: f64) {
        if !value.is_finite() {
            return;
        }
        if let Some(slot) = self.pending.iter_mut().find(|(pending, _)| *pending == id) {
            slot.1 = value;
        } else if self.pending.len() < MAX_PARAMS {
            self.pending.push((id, value));
        }
    }

    /// Runs `left` and `right` through the plugin, in place, [`Self::latency`] late. Never
    /// waits or allocates. Once the host failed, the sound passes unchanged.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        if self.health.failed.load(Ordering::Acquire) {
            return;
        }
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.gather.left.push(*l);
            self.gather.right.push(*r);
            if self.gather.left.len() >= BLOCK {
                self.send_gathered();
            }
            let (out_l, out_r) = self.next_frame();
            *l = out_l;
            *r = out_r;
        }
    }

    /// Forgets the sound so far (the host's plugin too).
    pub fn reset(&mut self) {
        // The blocks in flight play out; the plugin forgets its tails before the next
        // block it gets.
        self.reset = true;
    }

    fn send_gathered(&mut self) {
        let Ok(mut packet) = self.free.try_recv() else {
            // No packet free (the host is far behind): this block is lost.
            self.gather.left.clear();
            self.gather.right.clear();
            self.next_seq = self.next_seq.wrapping_add(1);
            return;
        };
        packet.seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        packet.left.clear();
        packet.right.clear();
        packet.left.extend_from_slice(&self.gather.left);
        packet.right.extend_from_slice(&self.gather.right);
        packet.params.clear();
        packet.params.append(&mut self.pending);
        packet.reset = std::mem::take(&mut self.reset);
        self.gather.left.clear();
        self.gather.right.clear();
        if let Err(mpsc::TrySendError::Full(packet) | mpsc::TrySendError::Disconnected(packet)) =
            self.outgoing.try_send(packet)
        {
            let _ = self.recycle.try_send(packet);
        }
    }

    /// The next frame of sound back from the host, or silence.
    fn next_frame(&mut self) -> (f32, f32) {
        if self.lead > 0 {
            self.lead -= 1;
            return (0.0, 0.0);
        }
        if self.playing.is_none() {
            self.playing = self.take_expected();
            self.played = 0;
        }
        let frame = match &self.playing {
            Some(packet) => (
                packet.left.get(self.played).copied().unwrap_or(0.0),
                packet.right.get(self.played).copied().unwrap_or(0.0),
            ),
            // Not back in time: silence for this block.
            None => (0.0, 0.0),
        };
        self.played += 1;
        if self.played >= BLOCK {
            if let Some(packet) = self.playing.take() {
                let _ = self.recycle.try_send(packet);
            }
            self.expected = self.expected.wrapping_add(1);
            self.played = 0;
        }
        frame
    }

    /// The block expected now, if it came; blocks older than it (too late) are dropped.
    fn take_expected(&mut self) -> Option<Packet> {
        loop {
            let packet = match self.early.take() {
                Some(packet) => packet,
                None if self.offline => match self.incoming.recv_timeout(WATCHDOG) {
                    Ok(packet) => packet,
                    Err(_) => return None,
                },
                None => match self.incoming.try_recv() {
                    Ok(packet) => packet,
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => return None,
                },
            };
            if packet.seq == self.expected {
                return Some(packet);
            }
            if packet.seq > self.expected {
                // The expected one was lost: its turn is silence, this one waits for its own.
                self.early = Some(packet);
                return None;
            }
            // Too late for its turn.
            let _ = self.recycle.try_send(packet);
        }
    }
}

impl Drop for RemoteEffect {
    fn drop(&mut self) {
        // Ending the channel ends the writer, which asks the host to quit; the host
        // closing its output ends the reader. A host that does not go is stopped.
        self.health.closing.store(true, Ordering::Release);
        let (dead_out, _) = mpsc::sync_channel(0);
        drop(std::mem::replace(&mut self.outgoing, dead_out));
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let done = self
                .child
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .try_wait()
                .map_or(true, |status| status.is_some());
            if done || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        stop(&mut self.child.lock().unwrap_or_else(PoisonError::into_inner));
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

/// The watchdog: stops a host that leaves blocks unanswered for [`WATCHDOG`] (which also
/// frees the writer, blocked on a full pipe, and the reader).
fn watch(health: &Health, child: &Mutex<Child>) {
    let mut answered = health.answered.load(Ordering::Relaxed);
    let mut since = Instant::now();
    while !health.closing.load(Ordering::Acquire) && !health.failed.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(50));
        let now_answered = health.answered.load(Ordering::Relaxed);
        if now_answered != answered {
            answered = now_answered;
            since = Instant::now();
        } else if health.sent.load(Ordering::Relaxed) > answered && since.elapsed() > WATCHDOG {
            health.fail("the plugin stopped answering and was stopped".into());
            stop(&mut child.lock().unwrap_or_else(PoisonError::into_inner));
            return;
        } else if health.sent.load(Ordering::Relaxed) <= answered {
            // Nothing waits for an answer (not playing): nothing to hold against it.
            since = Instant::now();
        }
    }
}

/// The writer thread: sends each block.
fn write_blocks(
    stdin: ChildStdin,
    blocks: &Receiver<Packet>,
    sent_seqs: &Sender<u64>,
    health: &Health,
    child: &Mutex<Child>,
) {
    let mut stdin = BufWriter::new(stdin);
    while let Ok(mut packet) = blocks.recv() {
        if packet.reset && write_frame(&mut stdin, &ToHost::Reset.encode()).is_err() {
            health.fail(died(child));
            return;
        }
        let message = ToHost::Process {
            params: std::mem::take(&mut packet.params),
            left: std::mem::take(&mut packet.left),
            right: std::mem::take(&mut packet.right),
        };
        // The reader labels the answers in the order the blocks went out.
        let _ = sent_seqs.send(packet.seq);
        health.sent.fetch_add(1, Ordering::Relaxed);
        if write_frame(&mut stdin, &message.encode()).is_err() {
            health.fail(died(child));
            return;
        }
    }
    let _ = write_frame(&mut stdin, &ToHost::Quit.encode());
    let _ = stdin.flush();
}

/// The reader thread: each answer into a free packet, on to the audio side.
fn read_blocks(
    stdout: BufReader<ChildStdout>,
    sent_seqs: &Receiver<u64>,
    used: &Receiver<Packet>,
    free: &SyncSender<Packet>,
    to_audio: &SyncSender<Packet>,
    health: &Health,
) {
    let mut stdout = stdout;
    let mut spare: VecDeque<Packet> = VecDeque::new();
    loop {
        let frame = match read_frame(&mut stdout) {
            Ok(Some(frame)) => frame,
            Ok(None) | Err(_) => {
                if !health.failed.load(Ordering::Acquire) {
                    health.fail("the plugin crashed and is bypassed".into());
                }
                return;
            }
        };
        health.answered.fetch_add(1, Ordering::Relaxed);
        let (left, right) = match FromHost::decode(&frame) {
            Ok(FromHost::Processed(Ok(block))) => block,
            Ok(FromHost::Processed(Err(why))) => {
                health.fail(format!("the plugin failed: {why}"));
                return;
            }
            Ok(_) | Err(_) => {
                health.fail("the plugin host answered nonsense and was stopped".into());
                return;
            }
        };
        // Packets the audio side used come back here and go to its free pool.
        while let Ok(packet) = used.try_recv() {
            spare.push_back(packet);
        }
        let Ok(seq) = sent_seqs.recv() else {
            return;
        };
        // A new packet for the answer (this thread may allocate); every packet the audio
        // side used goes back to its free pool, so the two stay balanced: the writer
        // keeps one per block sent, the audio side returns one per block played.
        let packet = Packet {
            seq,
            left,
            right,
            params: Vec::with_capacity(MAX_PARAMS),
            reset: false,
        };
        if to_audio.try_send(packet).is_err() {
            // The audio side is not taking blocks (stopped): nothing to do with it.
        }
        while let Some(packet) = spare.pop_front() {
            if let Err(
                mpsc::TrySendError::Full(packet) | mpsc::TrySendError::Disconnected(packet),
            ) = free.try_send(packet)
            {
                spare.push_front(packet);
                break;
            }
        }
    }
}

/// Starts a host program.
fn spawn() -> Result<(Child, ChildStdin, BufReader<ChildStdout>), String> {
    let mut child = host_command()?
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start the plugin host: {e}"))?;
    let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        stop(&mut child);
        return Err("the plugin host has no pipes".into());
    };
    Ok((child, stdin, BufReader::new(stdout)))
}

type Answer = (Result<FromHost, String>, BufReader<ChildStdout>);

/// Sends `message` and reads the answer on a thread of its own (handing the pipe back).
fn request(
    stdin: &mut ChildStdin,
    mut stdout: BufReader<ChildStdout>,
    message: &ToHost,
) -> Receiver<Answer> {
    let (reply, answer) = mpsc::sync_channel(1);
    if write_frame(stdin, &message.encode()).is_err() {
        // The answer thread below reads the end of the stream and says so.
    }
    let _ = std::thread::Builder::new()
        .name("plugin host answer".into())
        .spawn(move || {
            let decoded = match read_frame(&mut stdout) {
                Ok(Some(frame)) => FromHost::decode(&frame),
                Ok(None) => Err("the plugin host ended".to_owned()),
                Err(e) => Err(e.to_string()),
            };
            let _ = reply.send((decoded, stdout));
        });
    answer
}

/// Runs one request in a host of its own and returns its answer.
fn one_shot(message: &ToHost) -> Result<FromHost, String> {
    let (mut child, mut stdin, stdout) = spawn()?;
    let answer = request(&mut stdin, stdout, message);
    let result = match answer.recv_timeout(START_WAIT) {
        Ok((Ok(answer), _)) => Ok(answer),
        Ok((Err(_), _)) => Err(died_or_late(&mut child)),
        Err(_) => Err(died_or_late(&mut child)),
    };
    let _ = write_frame(&mut stdin, &ToHost::Quit.encode());
    drop(stdin);
    std::thread::sleep(Duration::from_millis(1));
    stop(&mut child);
    result
}

/// The plugin `id` of `library` with the settings `state` loaded: its description, its
/// parameters, their values and its latency (in a host of its own).
pub fn describe(
    library: &Path,
    id: &str,
    state: &[u8],
) -> Result<(PluginInfo, Vec<Param>, Vec<f64>, u32), PluginError> {
    let message = ToHost::Describe {
        library: library.to_owned(),
        id: id.to_owned(),
        state: state.to_owned(),
    };
    let fail = |reason: String| PluginError::Start {
        id: id.to_owned(),
        reason,
    };
    match one_shot(&message).map_err(fail)? {
        FromHost::Described(Ok(description)) => Ok(description),
        FromHost::Described(Err(why)) => Err(fail(why)),
        _ => Err(fail("the plugin host answered nonsense".into())),
    }
}

/// The plugins of `library` (in a host of its own: a library that crashes while it is
/// opened takes only that host, and is reported).
pub fn scan(library: &Path) -> Result<Vec<PluginInfo>, PluginError> {
    let fail = |reason: String| PluginError::Open {
        path: library.to_owned(),
        reason,
    };
    match one_shot(&ToHost::Scan {
        library: library.to_owned(),
    })
    .map_err(fail)?
    {
        FromHost::Scanned(Ok(plugins)) => Ok(plugins),
        FromHost::Scanned(Err(why)) => Err(fail(why)),
        _ => Err(fail("the plugin host answered nonsense".into())),
    }
}

/// Why a host gave no answer: it ended (crashed), or it is still running (too slow).
fn died_or_late(child: &mut Child) -> String {
    match child.try_wait() {
        Ok(Some(status)) => format!("the plugin crashed ({status})"),
        _ => "the plugin did not answer in time".into(),
    }
}

fn died(child: &Mutex<Child>) -> String {
    died_or_late(&mut child.lock().unwrap_or_else(PoisonError::into_inner))
}

/// Ends a host process (killed if it still runs) and collects it.
fn stop(child: &mut Child) {
    if matches!(child.try_wait(), Ok(None)) {
        let _ = child.kill();
    }
    let _ = child.wait();
}

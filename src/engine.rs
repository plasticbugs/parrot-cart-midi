//! The player itself, shared by the window and headless mode: it watches for
//! the cart's MIDI port, feeds what comes in to the synth or to a MIDI
//! output, and keeps the sound going.

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

use crate::midi::{self, Assembler};
use crate::mt32::{self, Mt32, RomSet};
use crate::paths;
use crate::ring::Ring;
use crate::synth::Synth;

const CLIENT: &str = "Parrot Cart MIDI";
const QUIET_RATE: u32 = 48_000;

pub type WavWriter = hound::WavWriter<BufWriter<File>>;

pub struct Config {
    /// Name patterns for the MIDI input (any part, any case).
    pub from: Vec<String>,
    /// A MIDI output to send to instead of the built-in synth.
    pub to: Option<String>,
    pub roms_cli: Option<PathBuf>,
    pub roms_chosen: Option<PathBuf>,
    pub mt32_only: bool,
    /// Render in real time but open no sound output.
    pub quiet: bool,
    pub wav: Option<PathBuf>,
    pub volume: f32,
    /// Watch for the cart (false while playing a test file).
    pub listen: bool,
}

#[derive(Clone, Default)]
pub struct Status {
    /// The MIDI input in use, if connected.
    pub input: Option<String>,
    /// The ROM set in use, if any.
    pub roms: Option<RomSummary>,
    /// Set once a ROM search found nothing: the folders it looked in.
    pub roms_missing: Option<Vec<PathBuf>>,
    /// The folder chosen in the window had no ROMs in it.
    pub chosen_empty: bool,
    pub synth_error: Option<String>,
    /// Where the music goes when not the built-in synth, and whether that
    /// output is there.
    pub output: Option<String>,
    pub output_connected: bool,
    pub audio: Option<String>,
    pub audio_error: Option<String>,
    pub midi_error: Option<String>,
    /// The MIDI outputs there are, for the window's menu.
    pub outputs: Vec<String>,
    pub lcd: String,
    pub led: bool,
}

#[derive(Clone)]
pub struct RomSummary {
    pub family: &'static str,
    pub folder: PathBuf,
    pub control: String,
}

pub struct Summary {
    pub seconds: f64,
    pub peak: f32,
}

pub enum Cmd {
    ChooseRoms(PathBuf),
    SetOutput(Option<String>),
    TestSound,
}

struct Shared {
    status: Mutex<Status>,
    ring: Ring,
    synth: Mutex<Option<Box<dyn Synth>>>,
    volume: AtomicU32,
    forwarding: AtomicBool,
    out: Mutex<Option<MidiOutputConnection>>,
    asm: Mutex<Assembler>,
    bytes_in: AtomicU64,
    quit: AtomicBool,
    peak: AtomicU32,
    frames: AtomicU64,
    wav: Mutex<Option<WavWriter>>,
    display: Mutex<(String, bool)>,
    stream_failed: AtomicBool,
}

impl Shared {
    fn status(&self) -> std::sync::MutexGuard<'_, Status> {
        self.status.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Sends MIDI into the player as if it came from the cart.
#[derive(Clone)]
pub struct Injector(Arc<Shared>);

impl Injector {
    pub fn send(&self, bytes: &[u8]) {
        deliver(&self.0, bytes);
    }
}

fn deliver(sh: &Shared, bytes: &[u8]) {
    sh.bytes_in.fetch_add(bytes.len() as u64, Relaxed);
    if sh.forwarding.load(Relaxed) {
        let mut asm = sh.asm.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = sh.out.lock().unwrap_or_else(|e| e.into_inner());
        asm.push(bytes, |m| {
            if let Some(c) = out.as_mut() {
                let _ = c.send(m);
            }
        });
    } else {
        sh.ring.push(bytes);
    }
}

pub struct Engine {
    shared: Arc<Shared>,
    cmds: Sender<Cmd>,
    lines: Receiver<String>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    pub fn start(config: Config) -> Result<Engine, String> {
        let shared = Arc::new(Shared {
            status: Mutex::new(Status::default()),
            ring: Ring::new(1 << 16),
            synth: Mutex::new(None),
            volume: AtomicU32::new(config.volume.to_bits()),
            forwarding: AtomicBool::new(config.to.is_some()),
            out: Mutex::new(None),
            asm: Mutex::new(Assembler::default()),
            bytes_in: AtomicU64::new(0),
            quit: AtomicBool::new(false),
            peak: AtomicU32::new(0),
            frames: AtomicU64::new(0),
            wav: Mutex::new(None),
            display: Mutex::new((String::new(), false)),
            stream_failed: AtomicBool::new(false),
        });
        let (cmds, cmd_rx) = channel();
        let (line_tx, lines) = channel();
        let sh = shared.clone();
        let thread = thread::Builder::new()
            .name("player".into())
            .spawn(move || run(sh, config, cmd_rx, line_tx))
            .map_err(|e| e.to_string())?;
        Ok(Engine { shared, cmds, lines, thread: Some(thread) })
    }

    pub fn status(&self) -> Status {
        let mut s = self.shared.status().clone();
        if let Ok(d) = self.shared.display.lock() {
            s.lcd = d.0.clone();
            s.led = d.1;
        }
        s
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.cmds.send(cmd);
    }

    pub fn set_volume(&self, v: f32) {
        self.shared.volume.store(v.clamp(0.0, 1.0).to_bits(), Relaxed);
    }

    /// Status lines for the terminal, since the last call.
    pub fn lines(&self) -> Vec<String> {
        self.lines.try_iter().collect()
    }

    pub fn injector(&self) -> Injector {
        Injector(self.shared.clone())
    }

    pub fn stop(mut self) -> Summary {
        self.shared.quit.store(true, Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        summary(&self.shared)
    }
}

fn summary(sh: &Shared) -> Summary {
    let rate = sh.synth.lock().ok().and_then(|s| s.as_ref().map(|s| s.sample_rate())).unwrap_or(QUIET_RATE);
    Summary { seconds: sh.frames.load(Relaxed) as f64 / f64::from(rate), peak: f32::from_bits(sh.peak.load(Relaxed)) }
}

// ---- the player thread -------------------------------------------------------------

enum Audio {
    Device(#[allow(dead_code)] cpal::Stream),
    Pacer(Arc<AtomicBool>, JoinHandle<()>),
}

impl Audio {
    fn stop(self) {
        match self {
            Audio::Device(stream) => drop(stream),
            Audio::Pacer(stop, handle) => {
                stop.store(true, Relaxed);
                let _ = handle.join();
            }
        }
    }
}

fn run(sh: Arc<Shared>, mut cfg: Config, cmds: Receiver<Cmd>, lines: Sender<String>) {
    let say = |s: String| {
        let _ = lines.send(s);
    };
    let (lcd_tx, lcd_rx) = channel::<String>();
    let mut audio: Option<Audio> = None;
    let mut audio_retry = Instant::now();
    let mut roms: Option<RomSet> = None;
    let mut want_roms = cfg.to.is_none();
    let mut input = InputWatch::default();
    let mut output = OutputWatch::default();
    let mut tune: Option<JoinHandle<()>> = None;

    sh.status().output = cfg.to.clone();

    while !sh.quit.load(Relaxed) {
        while let Ok(cmd) = cmds.try_recv() {
            match cmd {
                Cmd::ChooseRoms(dir) => {
                    cfg.roms_chosen = Some(dir);
                    cfg.roms_cli = None;
                    want_roms = true;
                }
                Cmd::SetOutput(to) => {
                    if to != cfg.to {
                        *sh.asm.lock().unwrap() = Assembler::default();
                        sh.forwarding.store(to.is_some(), Relaxed);
                        output.close(&sh);
                        if to.is_none() && roms.is_none() {
                            want_roms = true;
                        }
                        cfg.to = to;
                        let mut st = sh.status();
                        st.output = cfg.to.clone();
                        st.output_connected = false;
                    }
                }
                Cmd::TestSound => {
                    if tune.as_ref().is_none_or(|t| t.is_finished()) {
                        let sh = sh.clone();
                        tune = Some(thread::spawn(move || play_tune(&sh)));
                    }
                }
            }
        }

        // The built-in synth: find the ROMs, then start the sound.
        if want_roms {
            want_roms = false;
            let chosen = cfg.roms_chosen.clone();
            let found = load_roms(&sh, &cfg, &lcd_tx, &say, roms.as_ref(), audio_rate(&sh, &cfg));
            if let Some(set) = found {
                roms = Some(set);
            }
            if let Some(dir) = chosen {
                let empty = roms.as_ref().is_none_or(|r| r.folder != dir);
                sh.status().chosen_empty = empty;
            }
        }
        let have_synth = sh.synth.lock().map(|s| s.is_some()).unwrap_or(false);
        if have_synth {
            if sh.stream_failed.swap(false, Relaxed) {
                if let Some(a) = audio.take() {
                    a.stop();
                }
                say("The sound output stopped. Trying again.".into());
            }
            if audio.is_none() && Instant::now() >= audio_retry {
                match start_audio(&sh, &cfg) {
                    Ok((a, what)) => {
                        if let Some(what) = what {
                            say(format!("Sound output: {what}"));
                            sh.status().audio = Some(what);
                        }
                        sh.status().audio_error = None;
                        audio = Some(a);
                    }
                    Err(e) => {
                        let mut st = sh.status();
                        if st.audio_error.as_deref() != Some(&e) {
                            say(format!("No sound output: {e}. Trying again."));
                        }
                        st.audio_error = Some(e);
                        audio_retry = Instant::now() + Duration::from_secs(2);
                    }
                }
            }
        }

        if cfg.listen {
            input.poll(&sh, &cfg.from, &say);
        }
        output.poll(&sh, cfg.to.as_deref(), &say);

        for m in lcd_rx.try_iter() {
            say(format!("MT-32 display: {m}"));
        }
        pause(Duration::from_millis(40));
    }

    input.close(&sh);
    output.close(&sh);
    if let Some(a) = audio.take() {
        a.stop();
    }
    if let Some(w) = sh.wav.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = w.finalize();
    }
}

// Waits a moment. On macOS, CoreMIDI keeps its list of ports up to date by
// messages that arrive on the run loop of the thread that made the first
// MIDI client, which is this one, so it runs that loop meanwhile.
#[cfg(target_os = "macos")]
fn pause(d: Duration) {
    use std::ffi::c_void;
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFRunLoopDefaultMode: *const c_void;
        fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after_source_handled: u8) -> i32;
    }
    let start = Instant::now();
    unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, d.as_secs_f64(), 0) };
    if let Some(rest) = d.checked_sub(start.elapsed()) {
        thread::sleep(rest); // the loop had nothing to wait on
    }
}

#[cfg(not(target_os = "macos"))]
fn pause(d: Duration) {
    thread::sleep(d);
}

// The rate the synth should render at: the sound output's own rate, so no
// conversion happens after the synth's.
fn audio_rate(sh: &Shared, cfg: &Config) -> u32 {
    if let Ok(s) = sh.synth.lock()
        && let Some(s) = s.as_ref()
    {
        return s.sample_rate();
    }
    if cfg.quiet {
        return QUIET_RATE;
    }
    cpal::default_host()
        .default_output_device()
        .and_then(|d| d.default_output_config().ok())
        .map(|c| c.sample_rate())
        .unwrap_or(QUIET_RATE)
}

fn load_roms(
    sh: &Shared,
    cfg: &Config,
    lcd: &Sender<String>,
    say: &dyn Fn(String),
    current: Option<&RomSet>,
    rate: u32,
) -> Option<RomSet> {
    if let Some(d) = paths::data_dir() {
        let _ = std::fs::create_dir_all(d); // so it's there to put ROMs in
    }
    let folders = paths::rom_search_list(cfg.roms_cli.as_deref(), cfg.roms_chosen.as_deref());
    let Some(set) = mt32::find_roms(&folders, cfg.mt32_only) else {
        if current.is_none() {
            let mut st = sh.status();
            st.roms = None;
            st.roms_missing = Some(folders);
        }
        return None;
    };
    if let Some(cur) = current
        && cur.machine == set.machine
        && cur.folder == set.folder
    {
        return None; // the same set: keep playing
    }
    match Mt32::open(&set, rate, lcd.clone()) {
        Ok(synth) => {
            if let Some(path) = &cfg.wav {
                open_wav(sh, path, synth.sample_rate(), say);
            }
            *sh.synth.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(synth));
            say(format!("Sound: emulated {} ({}), ROMs from {}", set.family.name(), set.control, set.folder.display()));
            let mut st = sh.status();
            st.roms = Some(RomSummary {
                family: set.family.name(),
                folder: set.folder.clone(),
                control: set.control.clone(),
            });
            st.roms_missing = None;
            st.synth_error = None;
            Some(set)
        }
        Err(e) => {
            say(format!("The synth did not start: {e}"));
            sh.status().synth_error = Some(e);
            None
        }
    }
}

fn open_wav(sh: &Shared, path: &PathBuf, rate: u32, say: &dyn Fn(String)) {
    let mut w = sh.wav.lock().unwrap_or_else(|e| e.into_inner());
    if w.is_some() {
        return;
    }
    let spec =
        hound::WavSpec { channels: 2, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    match hound::WavWriter::create(path, spec) {
        Ok(writer) => *w = Some(writer),
        Err(e) => say(format!("Can't write {}: {e}", path.display())),
    }
}

fn play_tune(sh: &Shared) {
    let start = Instant::now();
    for (ms, msg) in midi::test_tune() {
        let at = start + Duration::from_millis(ms);
        if let Some(wait) = at.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }
        deliver(sh, &msg);
    }
}

// ---- rendering -------------------------------------------------------------------

struct Renderer {
    sh: Arc<Shared>,
    since_display: usize,
}

impl Renderer {
    /// Fills `out` (stereo, at the synth's rate) with what the synth plays.
    fn fill(&mut self, out: &mut [f32]) {
        let sh = &self.sh;
        match sh.synth.try_lock() {
            Ok(mut guard) => match guard.as_mut() {
                Some(s) => {
                    sh.ring.drain(|b| s.feed(b));
                    s.render(out);
                    let rate = s.sample_rate() as usize;
                    self.since_display += out.len() / 2;
                    if self.since_display >= rate / 30 {
                        self.since_display = 0;
                        if let Some(d) = s.display()
                            && let Ok(mut g) = sh.display.try_lock()
                        {
                            *g = d;
                        }
                    }
                }
                None => {
                    sh.ring.drain(|_| {});
                    out.fill(0.0);
                }
            },
            Err(_) => out.fill(0.0),
        }
        let vol = f32::from_bits(sh.volume.load(Relaxed));
        let mut peak = 0.0f32;
        for x in out.iter_mut() {
            *x *= vol;
            peak = peak.max(x.abs());
        }
        if peak > f32::from_bits(sh.peak.load(Relaxed)) {
            sh.peak.store(peak.to_bits(), Relaxed);
        }
        sh.frames.fetch_add((out.len() / 2) as u64, Relaxed);
        if let Ok(mut w) = sh.wav.try_lock()
            && let Some(w) = w.as_mut()
        {
            for x in out.iter() {
                let _ = w.write_sample((x.clamp(-1.0, 1.0) * 32767.0) as i16);
            }
        }
    }
}

/// Linear interpolation from the synth's rate to the output's, for when an
/// output can't run at the synth's rate. Not used when the rates match.
struct Resampler {
    step: f64,
    t: f64,
    a: [f32; 2],
    b: [f32; 2],
    buf: Vec<f32>,
    pos: usize,
    len: usize,
}

const CHUNK: usize = 64;

impl Resampler {
    fn new(from: u32, to: u32) -> Resampler {
        Resampler {
            step: f64::from(from) / f64::from(to),
            t: 1.0,
            a: [0.0; 2],
            b: [0.0; 2],
            buf: vec![0.0; CHUNK * 2],
            pos: 0,
            len: 0,
        }
    }

    fn produce(&mut self, frames: usize, r: &mut Renderer, out: &mut Vec<f32>) {
        out.clear();
        for _ in 0..frames {
            while self.t >= 1.0 {
                if self.pos == self.len {
                    r.fill(&mut self.buf);
                    self.pos = 0;
                    self.len = CHUNK;
                }
                self.a = self.b;
                self.b = [self.buf[self.pos * 2], self.buf[self.pos * 2 + 1]];
                self.pos += 1;
                self.t -= 1.0;
            }
            let t = self.t as f32;
            out.push(self.a[0] + (self.b[0] - self.a[0]) * t);
            out.push(self.a[1] + (self.b[1] - self.a[1]) * t);
            self.t += self.step;
        }
    }
}

fn start_audio(sh: &Arc<Shared>, cfg: &Config) -> Result<(Audio, Option<String>), String> {
    let rate = sh.synth.lock().ok().and_then(|s| s.as_ref().map(|s| s.sample_rate())).ok_or("no synth")?;
    if cfg.quiet {
        let stop = Arc::new(AtomicBool::new(false));
        let mut r = Renderer { sh: sh.clone(), since_display: 0 };
        let (stop2, sh2) = (stop.clone(), sh.clone());
        let handle = thread::spawn(move || {
            let start = Instant::now();
            let mut done: u64 = 0;
            let mut buf = vec![0.0f32; 2 * 512];
            while !stop2.load(Relaxed) && !sh2.quit.load(Relaxed) {
                let due = (start.elapsed().as_secs_f64() * f64::from(rate)) as u64;
                while done < due {
                    let n = (due - done).min(512) as usize;
                    r.fill(&mut buf[..n * 2]);
                    done += n as u64;
                }
                thread::sleep(Duration::from_millis(5));
            }
        });
        return Ok((Audio::Pacer(stop, handle), None));
    }

    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("none found")?;
    let name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "default".into());
    let def = device.default_output_config().map_err(|e| e.to_string())?;
    let format = def.sample_format();
    let mut config = def.config();
    // Run at the synth's rate when the output takes it; otherwise convert.
    let takes_rate = device
        .supported_output_configs()
        .map(|mut it| {
            it.any(|c| c.channels() == config.channels && c.sample_format() == format && c.contains_rate(rate))
        })
        .unwrap_or(false);
    if takes_rate {
        config.sample_rate = rate;
    }
    // About 10 ms per buffer, for low latency.
    let mut low = config;
    if let cpal::SupportedBufferSize::Range { min, max } = def.buffer_size() {
        low.buffer_size = cpal::BufferSize::Fixed((config.sample_rate / 100).clamp(*min, *max));
    }
    let stream = build(&device, low, format, sh, rate).or_else(|_| build(&device, config, format, sh, rate))?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((Audio::Device(stream), Some(name)))
}

fn build(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    format: cpal::SampleFormat,
    sh: &Arc<Shared>,
    rate: u32,
) -> Result<cpal::Stream, String> {
    use cpal::SampleFormat as F;
    match format {
        F::F32 => build_typed::<f32>(device, config, sh, rate),
        F::F64 => build_typed::<f64>(device, config, sh, rate),
        F::I16 => build_typed::<i16>(device, config, sh, rate),
        F::U16 => build_typed::<u16>(device, config, sh, rate),
        F::I32 => build_typed::<i32>(device, config, sh, rate),
        F::U32 => build_typed::<u32>(device, config, sh, rate),
        F::I8 => build_typed::<i8>(device, config, sh, rate),
        F::U8 => build_typed::<u8>(device, config, sh, rate),
        other => Err(format!("its sample format ({other}) is not supported")),
    }
}

fn build_typed<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    sh: &Arc<Shared>,
    rate: u32,
) -> Result<cpal::Stream, String> {
    let channels = usize::from(config.channels).max(1);
    let mut r = Renderer { sh: sh.clone(), since_display: 0 };
    let mut resampler = (config.sample_rate != rate).then(|| Resampler::new(rate, config.sample_rate));
    let mut stereo: Vec<f32> = Vec::with_capacity(8192);
    let failed = sh.clone();
    device
        .build_output_stream::<T, _, _>(
            config,
            move |data: &mut [T], _| {
                let frames = data.len() / channels;
                match resampler.as_mut() {
                    Some(rs) => rs.produce(frames, &mut r, &mut stereo),
                    None => {
                        stereo.resize(frames * 2, 0.0);
                        r.fill(&mut stereo);
                    }
                }
                for (frame, lr) in data.chunks_mut(channels).zip(stereo.chunks(2)) {
                    if channels == 1 {
                        frame[0] = T::from_sample((lr[0] + lr[1]) * 0.5);
                    } else {
                        frame[0] = T::from_sample(lr[0]);
                        frame[1] = T::from_sample(lr[1]);
                        for c in &mut frame[2..] {
                            *c = T::from_sample(0.0);
                        }
                    }
                }
            },
            move |err| {
                if !matches!(err.kind(), cpal::ErrorKind::Xrun) {
                    failed.stream_failed.store(true, Relaxed);
                }
            },
            None,
        )
        .map_err(|e| e.to_string())
}

// ---- MIDI in and out ---------------------------------------------------------------

#[derive(Default)]
struct InputWatch {
    scanner: Option<MidiInput>,
    conn: Option<(MidiInputConnection<()>, String)>,
    next: Option<Instant>,
    said_waiting: bool,
    bytes_at_connect: u64,
    said_receiving: bool,
}

impl InputWatch {
    fn poll(&mut self, sh: &Arc<Shared>, patterns: &[String], say: &dyn Fn(String)) {
        if self.conn.is_some() && !self.said_receiving && sh.bytes_in.load(Relaxed) > self.bytes_at_connect {
            self.said_receiving = true;
            say("Receiving music.".into());
        }
        let now = Instant::now();
        if self.next.is_some_and(|n| now < n) {
            return;
        }
        self.next = Some(now + Duration::from_millis(500));
        if self.scanner.is_none() {
            match MidiInput::new(CLIENT) {
                Ok(m) => {
                    self.scanner = Some(m);
                    sh.status().midi_error = None;
                }
                Err(e) => {
                    let msg = format!("MIDI is not available on this computer ({e})");
                    let mut st = sh.status();
                    if st.midi_error.as_deref() != Some(&msg) {
                        say(msg.clone());
                    }
                    st.midi_error = Some(msg);
                    self.next = Some(now + Duration::from_secs(5));
                    return;
                }
            }
        }
        let scanner = self.scanner.as_ref().unwrap();
        let found = scanner.ports().into_iter().find_map(|p| {
            let name = scanner.port_name(&p).ok()?;
            midi::name_matches(&name, patterns).then_some((p, name))
        });
        if let Some((_, id)) = &self.conn {
            if found.as_ref().is_some_and(|(p, _)| p.id() == *id) {
                return; // still there
            }
            self.close(sh);
            say("The cart went away. Waiting for it to come back.".into());
            self.said_waiting = true;
        }
        let Some((port, name)) = found else {
            if !self.said_waiting {
                self.said_waiting = true;
                say("Waiting for the cart. Plug its USB-C port into this computer and start the game.".into());
            }
            return;
        };
        let mut mi = match MidiInput::new(CLIENT) {
            Ok(m) => m,
            Err(_) => return,
        };
        mi.ignore(Ignore::None);
        let sh2 = sh.clone();
        match mi.connect(&port, "in", move |_, bytes, _| deliver(&sh2, bytes), ()) {
            Ok(conn) => {
                self.conn = Some((conn, port.id()));
                self.said_waiting = false;
                self.said_receiving = false;
                self.bytes_at_connect = sh.bytes_in.load(Relaxed);
                say(format!("Connected to {name}."));
                sh.status().input = Some(name);
            }
            Err(e) => say(format!("Could not open {name}: {e}")),
        }
    }

    fn close(&mut self, sh: &Shared) {
        if let Some((conn, _)) = self.conn.take() {
            conn.close();
        }
        sh.status().input = None;
    }
}

#[derive(Default)]
struct OutputWatch {
    scanner: Option<MidiOutput>,
    conn: Option<(String, String)>, // (port id, name)
    next: Option<Instant>,
    said_waiting: bool,
}

impl OutputWatch {
    fn poll(&mut self, sh: &Arc<Shared>, target: Option<&str>, say: &dyn Fn(String)) {
        let now = Instant::now();
        if self.next.is_some_and(|n| now < n) {
            return;
        }
        self.next = Some(now + Duration::from_millis(1000));
        if self.scanner.is_none() {
            match MidiOutput::new(CLIENT) {
                Ok(m) => self.scanner = Some(m),
                Err(_) => return,
            }
        }
        let scanner = self.scanner.as_ref().unwrap();
        let cart: Vec<String> = midi::CART_NAMES.iter().map(|s| s.to_string()).collect();
        let ports: Vec<_> =
            scanner.ports().into_iter().filter_map(|p| scanner.port_name(&p).ok().map(|n| (p, n))).collect();
        sh.status().outputs = ports.iter().map(|(_, n)| n.clone()).filter(|n| !midi::name_matches(n, &cart)).collect();

        let Some(target) = target else { return };
        let want = vec![target.to_string()];
        let found = ports
            .iter()
            .find(|(_, n)| n == target)
            .or_else(|| ports.iter().find(|(_, n)| midi::name_matches(n, &want)));
        if let Some((id, name)) = &self.conn {
            if found.is_some_and(|(p, _)| p.id() == *id) {
                return;
            }
            let name = name.clone();
            self.close(sh);
            say(format!("{name} went away. Waiting for it to come back."));
            self.said_waiting = true;
        }
        let Some((port, name)) = found else {
            if !self.said_waiting {
                self.said_waiting = true;
                say(format!("Waiting for the MIDI output \"{target}\"."));
            }
            return;
        };
        let Ok(mo) = MidiOutput::new(CLIENT) else { return };
        match mo.connect(port, "out") {
            Ok(conn) => {
                *sh.out.lock().unwrap_or_else(|e| e.into_inner()) = Some(conn);
                self.conn = Some((port.id(), name.clone()));
                self.said_waiting = false;
                say(format!("Sending the music to {name}."));
                sh.status().output_connected = true;
            }
            Err(e) => say(format!("Could not open {name}: {e}")),
        }
    }

    fn close(&mut self, sh: &Shared) {
        if let Some(conn) = sh.out.lock().unwrap_or_else(|e| e.into_inner()).take() {
            conn.close();
        }
        self.conn = None;
        self.said_waiting = false;
        sh.status().output_connected = false;
    }
}

// ---- a test file, rendered as fast as it goes --------------------------------------

/// Plays timed MIDI into the built-in synth with no sound output, as fast
/// as the computer can, writing a WAV if asked. For tests.
pub fn render_offline(
    events: &[crate::midifile::Event],
    roms: &RomSet,
    wav: Option<&PathBuf>,
    mut say: impl FnMut(String),
) -> Result<Summary, String> {
    let (tx, rx) = channel();
    let mut synth = Mt32::open(roms, QUIET_RATE, tx)?;
    let rate = synth.sample_rate();
    let mut writer = match wav {
        Some(p) => {
            let spec = hound::WavSpec {
                channels: 2,
                sample_rate: rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            Some(hound::WavWriter::create(p, spec).map_err(|e| format!("can't write {}: {e}", p.display()))?)
        }
        None => None,
    };
    let mut buf = vec![0.0f32; 2 * 1024];
    let mut done: u64 = 0;
    let mut peak = 0.0f32;
    let mut render_to = |target: u64, synth: &mut Mt32, done: &mut u64| {
        while *done < target {
            let n = (target - *done).min(1024) as usize;
            synth.render(&mut buf[..n * 2]);
            for &x in &buf[..n * 2] {
                peak = peak.max(x.abs());
                if let Some(w) = writer.as_mut() {
                    let _ = w.write_sample((x.clamp(-1.0, 1.0) * 32767.0) as i16);
                }
            }
            *done += n as u64;
            for m in rx.try_iter() {
                say(format!("{:>7.2} s  MT-32 display: {m}", *done as f64 / f64::from(rate)));
            }
        }
    };
    for ev in events {
        let at = (ev.time * f64::from(rate)) as u64;
        render_to(at, &mut synth, &mut done);
        synth.feed(&ev.bytes);
    }
    // Two more seconds for the last notes and the reverb to die away.
    render_to(done + 2 * u64::from(rate), &mut synth, &mut done);
    drop(render_to);
    if let Some(w) = writer {
        w.finalize().map_err(|e| e.to_string())?;
    }
    Ok(Summary { seconds: done as f64 / f64::from(rate), peak })
}

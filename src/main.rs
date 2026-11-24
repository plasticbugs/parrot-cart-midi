// Parrot Cart MIDI: plays the Parrot Cart's MIDI music on this computer, on
// an emulated Roland MT-32 (Munt's libmt32emu) or a real one.

// A window app on Windows: no console window opens when it is double-clicked.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod engine;
mod gui;
mod midi;
mod midifile;
mod mt32;
mod paths;
mod ring;
mod synth;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use engine::{Config, Engine};
use paths::Settings;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
Plays the Parrot Cart's MIDI music on this computer, on an emulated Roland
MT-32 or CM-32L, or sends it to a real one.

Usage: parrot-midi [options]

With no options it opens a window and waits for the cart.

Options:
  --headless        No window. Status goes to the terminal. Ctrl-C ends it.
  --list            List the MIDI inputs and outputs, then quit.
  --from NAME       Listen to the MIDI input whose name contains NAME,
                    instead of the cart (\"Parrot Cart\").
  --to NAME         Send the music to the MIDI output whose name contains
                    NAME (a real MT-32 on a USB MIDI cable). No ROMs needed.
  --roms FOLDER     Look for the ROMs in FOLDER first.
  --mt32            Use MT-32 ROMs even when CM-32L ROMs are there.
  --wav FILE        Also write what it plays to FILE (a WAV file).
  --quiet           Make the sound but don't play it (for tests).
  --test-file FILE  Play a MIDI file (.mid) instead of listening for the cart.
                    With --quiet, it runs as fast as it can.
  --version         Show the version.
  --help            Show this help.

--wav, --quiet and --test-file imply --headless.

ROMs are found by their contents, not their names. It looks in --roms, the
folder chosen in the window, a \"roms\" folder next to the program, and:
  macOS    ~/Library/Application Support/Parrot Cart MIDI
  Windows  %APPDATA%\\Parrot Cart MIDI
  Linux    ~/.local/share/parrot-cart-midi
";

#[derive(Default)]
struct Options {
    headless: bool,
    list: bool,
    from: Option<String>,
    to: Option<String>,
    roms: Option<PathBuf>,
    mt32: bool,
    wav: Option<PathBuf>,
    quiet: bool,
    test_file: Option<PathBuf>,
    version: bool,
    help: bool,
}

fn parse(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut o = Options::default();
    let mut args = args.peekable();
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match a.as_str() {
            "--headless" => o.headless = true,
            "--list" | "-l" => o.list = true,
            "--from" => o.from = Some(value("--from")?),
            "--to" => o.to = Some(value("--to")?),
            "--roms" => o.roms = Some(PathBuf::from(value("--roms")?)),
            "--mt32" => o.mt32 = true,
            "--wav" => o.wav = Some(PathBuf::from(value("--wav")?)),
            "--quiet" | "-q" => o.quiet = true,
            "--test-file" => o.test_file = Some(PathBuf::from(value("--test-file")?)),
            "--version" | "-V" => o.version = true,
            "--help" | "-h" | "/?" => o.help = true,
            // Older macOS adds this when an app is opened from the Finder.
            s if s.starts_with("-psn_") => {}
            other => return Err(format!("unknown option: {other}")),
        }
    }
    if o.wav.is_some() || o.quiet || o.test_file.is_some() {
        o.headless = true;
    }
    Ok(o)
}

fn main() -> ExitCode {
    let opts = match parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(e) => {
            attach_console();
            eprintln!("parrot-midi: {e}\nTry parrot-midi --help.");
            return ExitCode::from(2);
        }
    };
    if opts.help || opts.version || opts.list || opts.headless {
        attach_console();
    }
    if opts.help {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if opts.version {
        println!("parrot-midi {VERSION} (libmt32emu {})", mt32::library_version());
        return ExitCode::SUCCESS;
    }
    if opts.list {
        return list();
    }

    let settings = Settings::load();
    let config = Config {
        from: match &opts.from {
            Some(f) => vec![f.clone()],
            None => midi::CART_NAMES.iter().map(|s| s.to_string()).collect(),
        },
        to: opts.to.clone(),
        roms_cli: opts.roms.clone(),
        roms_chosen: settings.rom_folder.clone(),
        mt32_only: opts.mt32,
        quiet: opts.quiet,
        wav: opts.wav.clone(),
        volume: 1.0,
        listen: opts.test_file.is_none(),
    };
    if opts.headless {
        return headless(&opts, config);
    }
    match gui::run(config, settings) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            attach_console();
            eprintln!("parrot-midi: the window did not open ({e}).\nTo run without a window, use --headless.");
            ExitCode::FAILURE
        }
    }
}

// A Windows GUI program has no console; when started from one with
// --headless (or --list, --help, ...), print to that console. Output that
// is already redirected (to a file or a pipe) stays where it is.
#[cfg(windows)]
fn attach_console() {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn GetStdHandle(which: u32) -> *mut c_void;
        fn GetFileType(handle: *mut c_void) -> u32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const FILE_TYPE_UNKNOWN: u32 = 0;
    unsafe {
        let out = GetStdHandle(STD_OUTPUT_HANDLE);
        if out.is_null() || out as isize == -1 || GetFileType(out) == FILE_TYPE_UNKNOWN {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

#[cfg(not(windows))]
fn attach_console() {}

fn list() -> ExitCode {
    match midir::MidiInput::new("Parrot Cart MIDI") {
        Ok(m) => {
            println!("MIDI inputs:");
            for p in m.ports() {
                println!("  {}", m.port_name(&p).unwrap_or_default());
            }
        }
        Err(e) => println!("MIDI inputs: not available ({e})"),
    }
    match midir::MidiOutput::new("Parrot Cart MIDI") {
        Ok(m) => {
            println!("MIDI outputs:");
            for p in m.ports() {
                println!("  {}", m.port_name(&p).unwrap_or_default());
            }
        }
        Err(e) => println!("MIDI outputs: not available ({e})"),
    }
    ExitCode::SUCCESS
}

fn no_roms(folders: &[PathBuf]) {
    println!("No CM-32L or MT-32 ROMs found. Looked in:");
    for f in folders {
        println!("  {}", f.display());
    }
    println!(
        "Put the two ROM files from your CM-32L or MT-32 (the control ROM and the\n\
         PCM ROM) in one of these folders, or name a folder with --roms FOLDER."
    );
}

fn percent(peak: f32) -> String {
    if peak < 1e-5 {
        return "silence".into();
    }
    format!("{:.0}% ({:.1} dBFS)", peak * 100.0, 20.0 * peak.log10())
}

fn headless(opts: &Options, config: Config) -> ExitCode {
    println!("Parrot Cart MIDI {VERSION} (libmt32emu {})", mt32::library_version());

    let events = match &opts.test_file {
        Some(f) => match midifile::load(f) {
            Ok(ev) => Some(ev),
            Err(e) => {
                eprintln!("parrot-midi: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };

    // A test file with no sound and no MIDI output: render it at full speed.
    if let (Some(events), true, None) = (&events, opts.quiet, &opts.to) {
        let folders = paths::rom_search_list(config.roms_cli.as_deref(), config.roms_chosen.as_deref());
        let Some(roms) = mt32::find_roms(&folders, config.mt32_only) else {
            no_roms(&folders);
            return ExitCode::FAILURE;
        };
        println!("Sound: emulated {} ({}), ROMs from {}", roms.family.name(), roms.control, roms.folder.display());
        let length = events.last().map(|e| e.time).unwrap_or(0.0);
        println!(
            "Playing {} ({length:.1} s) as fast as it goes, with no sound.",
            opts.test_file.as_ref().unwrap().display()
        );
        let started = Instant::now();
        return match engine::render_offline(events, &roms, config.wav.as_ref(), |l| println!("{l}")) {
            Ok(s) => {
                println!(
                    "Rendered {:.1} s in {:.1} s. Loudest sample: {}.",
                    s.seconds,
                    started.elapsed().as_secs_f64(),
                    percent(s.peak)
                );
                if let Some(w) = &config.wav {
                    println!("Wrote {}.", w.display());
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("parrot-midi: {e}");
                ExitCode::FAILURE
            }
        };
    }

    let wav = config.wav.clone();
    let quiet = config.quiet;
    let forwarding = config.to.is_some();
    let engine = match Engine::start(config) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("parrot-midi: {e}");
            return ExitCode::FAILURE;
        }
    };
    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        let _ = ctrlc::set_handler(move || stop.store(true, Ordering::SeqCst));
    }

    // Play the test file in real time, as if the cart sent it.
    let done = Arc::new(AtomicBool::new(false));
    if let Some(events) = events {
        let (inject, done, stop) = (engine.injector(), done.clone(), stop.clone());
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(500)); // the synth and sound start
            let start = Instant::now();
            for ev in &events {
                let at = start + Duration::from_secs_f64(ev.time);
                while Instant::now() < at {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    thread::sleep((at - Instant::now()).min(Duration::from_millis(50)));
                }
                inject.send(&ev.bytes);
            }
            thread::sleep(Duration::from_secs(2));
            done.store(true, Ordering::SeqCst);
        });
    }

    loop {
        if !forwarding && let Some(folders) = engine.status().roms_missing {
            no_roms(&folders);
            engine.stop();
            return ExitCode::FAILURE;
        }
        for l in engine.lines() {
            println!("{l}");
        }
        if stop.load(Ordering::SeqCst) || done.load(Ordering::SeqCst) {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let summary = engine.stop();
    if stop.load(Ordering::SeqCst) {
        println!("Stopped.");
    }
    if !forwarding && (quiet || wav.is_some()) {
        println!("Rendered {:.1} s. Loudest sample: {}.", summary.seconds, percent(summary.peak));
    }
    if let Some(w) = wav {
        println!("Wrote {}.", w.display());
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> impl Iterator<Item = String> {
        s.split_whitespace().map(String::from).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn test_options_imply_headless() {
        let o = parse(args("--quiet --wav out.wav --roms r")).unwrap();
        assert!(o.headless && o.quiet);
        assert_eq!(o.roms, Some(PathBuf::from("r")));
        assert!(!parse(args("--to MT-32")).unwrap().headless);
        assert!(parse(args("--from")).is_err());
        assert!(parse(args("--bogus")).is_err());
    }
}

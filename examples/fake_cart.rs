//! Pretends to be the cart, to test the player without one: makes a virtual
//! MIDI source with a cart-like name and plays a MIDI file into it in real
//! time. With --sink, it is a virtual MIDI output instead, and counts what
//! arrives (to test --to). macOS and Linux only.
//!
//!   cargo run --example fake_cart -- [--name NAME] [--times N] [--seconds S] song.mid
//!   cargo run --example fake_cart -- --sink NAME [--seconds S]
//!
//! --times N opens the port, plays, and closes it N times, with a pause in
//! between, the way the cart comes and goes.

#[cfg(unix)]
#[path = "../src/midifile.rs"]
mod midifile;

#[cfg(not(unix))]
fn main() {
    eprintln!("fake_cart needs virtual MIDI ports, which only macOS and Linux have.");
}

#[cfg(unix)]
fn main() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use midir::os::unix::{VirtualInput, VirtualOutput};
    use midir::{MidiInput, MidiOutput};

    let mut name = "Parrot Cart MIDI (test)".to_string();
    let mut sink = None;
    let mut times = 1;
    let mut seconds = f64::INFINITY;
    let mut file = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--name" => name = args.next().expect("--name NAME"),
            "--sink" => sink = Some(args.next().expect("--sink NAME")),
            "--times" => times = args.next().and_then(|v| v.parse().ok()).expect("--times N"),
            "--seconds" => seconds = args.next().and_then(|v| v.parse().ok()).expect("--seconds S"),
            f => file = Some(std::path::PathBuf::from(f)),
        }
    }

    if let Some(sink) = sink {
        let (messages, bytes, sysex, notes) = (
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
        );
        let (m, b, s, n) = (messages.clone(), bytes.clone(), sysex.clone(), notes.clone());
        let mut input = MidiInput::new("fake cart").expect("MIDI");
        input.ignore(midir::Ignore::None);
        let _conn = input
            .create_virtual(
                &sink,
                move |_, data, _| {
                    m.fetch_add(1, Ordering::Relaxed);
                    b.fetch_add(data.len(), Ordering::Relaxed);
                    if data.first() == Some(&0xF0) {
                        s.fetch_add(1, Ordering::Relaxed);
                    }
                    if data.first().is_some_and(|&st| st & 0xF0 == 0x90 && data.get(2).is_some_and(|&v| v > 0)) {
                        n.fetch_add(1, Ordering::Relaxed);
                    }
                },
                (),
            )
            .expect("virtual input");
        println!("sink \"{sink}\" is open");
        sleep(Duration::from_secs_f64(if seconds.is_finite() { seconds } else { 30.0 }));
        println!(
            "sink got {} messages, {} bytes, {} SysEx, {} notes",
            messages.load(Ordering::Relaxed),
            bytes.load(Ordering::Relaxed),
            sysex.load(Ordering::Relaxed),
            notes.load(Ordering::Relaxed)
        );
        return;
    }

    let file = file.expect("a MIDI file to play");
    let events = midifile::load(&file).unwrap_or_else(|e| panic!("{e}"));
    for round in 1..=times {
        let out = MidiOutput::new("fake cart").expect("MIDI");
        let mut conn = out.create_virtual(&name).expect("virtual output");
        println!("round {round}: \"{name}\" is there");
        sleep(Duration::from_secs(2)); // time for the player to find it
        let start = Instant::now();
        let mut sent = 0;
        for ev in events.iter().take_while(|e| e.time <= seconds) {
            let at = start + Duration::from_secs_f64(ev.time);
            if let Some(wait) = at.checked_duration_since(Instant::now()) {
                sleep(wait);
            }
            let _ = conn.send(&ev.bytes);
            sent += 1;
        }
        // All notes off on every channel, as the cart's SCUMM plugin does on exit.
        for ch in 0..16u8 {
            let _ = conn.send(&[0xB0 | ch, 123, 0]);
        }
        sleep(Duration::from_millis(500));
        drop(conn);
        println!("round {round}: sent {sent} messages, \"{name}\" is gone");
        if round < times {
            sleep(Duration::from_secs(3));
        }
    }
}

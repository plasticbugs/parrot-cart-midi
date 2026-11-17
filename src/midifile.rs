//! Reads a Standard MIDI File (.mid) into timed MIDI messages, for
//! `--test-file` and the fake-cart example.

use std::path::Path;

use midly::{MetaMessage, Smf, Timing, TrackEventKind};

pub struct Event {
    /// Seconds from the start.
    pub time: f64,
    pub bytes: Vec<u8>,
}

pub fn load(path: &Path) -> Result<Vec<Event>, String> {
    let data = std::fs::read(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    parse(&data).map_err(|e| format!("{} is not a MIDI file I can read: {e}", path.display()))
}

pub fn parse(data: &[u8]) -> Result<Vec<Event>, String> {
    let smf = Smf::parse(data).map_err(|e| e.to_string())?;

    // Every track's events at their absolute tick, merged in time order. Ties
    // keep track order, so tempo changes in track 0 come first.
    let mut all = Vec::new();
    for (track_no, track) in smf.tracks.iter().enumerate() {
        let mut tick: u64 = 0;
        for (i, ev) in track.iter().enumerate() {
            tick += u64::from(ev.delta.as_int());
            all.push((tick, track_no, i, ev.kind));
        }
    }
    all.sort_by_key(|&(tick, track, i, _)| (tick, track, i));

    let mut events = Vec::new();
    let mut time = 0.0;
    let mut last_tick = 0u64;
    let mut tempo = 500_000.0; // microseconds per quarter note
    for (tick, _, _, kind) in all {
        let seconds_per_tick = match smf.header.timing {
            Timing::Metrical(tpq) => tempo / 1e6 / f64::from(tpq.as_int().max(1)),
            Timing::Timecode(fps, sub) => 1.0 / (f64::from(fps.as_f32()) * f64::from(sub.max(1))),
        };
        time += (tick - last_tick) as f64 * seconds_per_tick;
        last_tick = tick;
        let mut bytes = Vec::new();
        match kind {
            TrackEventKind::Meta(MetaMessage::Tempo(t)) => {
                tempo = f64::from(t.as_int());
                continue;
            }
            TrackEventKind::Meta(_) => continue,
            TrackEventKind::Midi { .. } => {
                if let Some(live) = kind.as_live_event() {
                    live.write_std(&mut bytes).map_err(|e| e.to_string())?;
                }
            }
            // A SysEx event leaves out the F0; a split one, or an escape,
            // goes as it is.
            TrackEventKind::SysEx(data) => {
                bytes.push(0xF0);
                bytes.extend_from_slice(data);
            }
            TrackEventKind::Escape(data) => bytes.extend_from_slice(data),
        }
        if !bytes.is_empty() {
            events.push(Event { time, bytes });
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A one-track file: 96 ticks per quarter, tempo 1 s per quarter, a
    // SysEx, a note on at 0 and its note off a quarter later.
    #[test]
    fn reads_times_and_messages() {
        let track: &[u8] = &[
            0x00, 0xFF, 0x51, 0x03, 0x0F, 0x42, 0x40, // tempo 1,000,000 us
            0x00, 0xF0, 0x04, 0x41, 0x10, 0x16, 0xF7, // SysEx
            0x00, 0x91, 60, 100, // note on, channel 2
            0x60, 0x81, 60, 0, // 96 ticks later: note off
            0x00, 0xFF, 0x2F, 0x00,
        ];
        let mut smf = b"MThd\0\0\0\x06\0\0\0\x01\0\x60MTrk".to_vec();
        smf.extend_from_slice(&(track.len() as u32).to_be_bytes());
        smf.extend_from_slice(track);
        let ev = parse(&smf).unwrap();
        assert_eq!(ev.len(), 3);
        assert_eq!(ev[0].bytes, [0xF0, 0x41, 0x10, 0x16, 0xF7]);
        assert_eq!(ev[1].bytes, [0x91, 60, 100]);
        assert_eq!(ev[2].bytes, [0x81, 60, 0]);
        assert!((ev[2].time - 1.0).abs() < 1e-9);
    }
}

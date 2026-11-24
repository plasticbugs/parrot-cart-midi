//! Small MIDI helpers: which ports are the cart, whole messages out of a
//! byte stream, and a short test tune.

/// The cart's MIDI port name. Any port whose name contains it, in any case,
/// is taken to be the cart.
pub const CART_NAMES: &[&str] = &["Parrot Cart"];

pub fn name_matches(name: &str, patterns: &[String]) -> bool {
    let name = name.to_lowercase();
    patterns.iter().any(|p| name.contains(&p.to_lowercase()))
}

/// Turns a raw MIDI byte stream (running status, SysEx split anywhere,
/// real-time bytes in between) into whole messages, as MIDI output APIs
/// such as WinMM want them.
#[derive(Default)]
pub struct Assembler {
    running: u8,
    msg: Vec<u8>,
    need: usize,
    sysex: bool,
}

const MAX_SYSEX: usize = 64 * 1024;

fn data_bytes(status: u8) -> usize {
    match status & 0xF0 {
        0xC0 | 0xD0 => 1,
        0x80..=0xE0 => 2,
        _ => match status {
            0xF1 | 0xF3 => 1,
            0xF2 => 2,
            _ => 0,
        },
    }
}

impl Assembler {
    pub fn push(&mut self, bytes: &[u8], mut out: impl FnMut(&[u8])) {
        for &b in bytes {
            if b >= 0xF8 {
                out(&[b]); // real-time: may come anywhere, even inside SysEx
            } else if b == 0xF0 {
                self.msg.clear();
                self.msg.push(b);
                self.sysex = true;
                self.running = 0;
            } else if b == 0xF7 {
                if self.sysex {
                    self.msg.push(b);
                    out(&self.msg);
                }
                self.msg.clear();
                self.sysex = false;
            } else if b & 0x80 != 0 {
                self.sysex = false;
                self.msg.clear();
                self.msg.push(b);
                self.running = if b < 0xF0 { b } else { 0 };
                self.need = data_bytes(b);
                if self.need == 0 {
                    out(&self.msg);
                    self.msg.clear();
                }
            } else if self.sysex {
                if self.msg.len() < MAX_SYSEX {
                    self.msg.push(b);
                }
            } else {
                if self.msg.is_empty() {
                    if self.running == 0 {
                        continue; // stray data byte
                    }
                    self.msg.push(self.running);
                    self.need = data_bytes(self.running);
                }
                self.msg.push(b);
                if self.msg.len() > self.need {
                    out(&self.msg);
                    self.msg.clear();
                }
            }
        }
    }
}

/// A short tune for "Test sound": a C major arpeggio and chord on the
/// MT-32's first part (MIDI channel 2), as (milliseconds, message).
pub fn test_tune() -> Vec<(u64, Vec<u8>)> {
    let mut tune = vec![(0, vec![0xC1, 0x00])]; // Acoustic Piano 1
    let notes = [60u8, 64, 67, 72];
    let mut t = 0;
    for n in notes {
        tune.push((t, vec![0x91, n, 100]));
        tune.push((t + 220, vec![0x81, n, 0]));
        t += 250;
    }
    for n in notes {
        tune.push((t, vec![0x91, n, 90]));
    }
    for n in notes {
        tune.push((t + 900, vec![0x81, n, 0]));
    }
    tune.sort_by_key(|(t, _)| *t);
    tune
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assemble(chunks: &[&[u8]]) -> Vec<Vec<u8>> {
        let mut a = Assembler::default();
        let mut got = Vec::new();
        for c in chunks {
            a.push(c, |m| got.push(m.to_vec()));
        }
        got
    }

    #[test]
    fn whole_messages_from_pieces() {
        let got = assemble(&[&[0x91, 60], &[100, 64, 100], &[0xF0, 0x41], &[0xF8, 0x10, 0xF7, 0xC1, 5]]);
        assert_eq!(
            got,
            vec![vec![0x91, 60, 100], vec![0x91, 64, 100], vec![0xF8], vec![0xF0, 0x41, 0x10, 0xF7], vec![0xC1, 5]]
        );
    }

    #[test]
    fn names() {
        let p: Vec<String> = CART_NAMES.iter().map(|s| s.to_string()).collect();
        assert!(name_matches("Parrot Cart MIDI", &p));
        assert!(name_matches("PARROT CART MIDI:Parrot Cart MIDI MIDI 1 20:0", &p));
        assert!(!name_matches("IAC Driver Bus 1", &p));
    }
}

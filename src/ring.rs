//! A byte queue from the MIDI threads to the audio thread. Writers take a
//! lock (they are not real-time); the one reader, the audio callback, never
//! blocks.

use std::cell::UnsafeCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Ring {
    buf: Box<[UnsafeCell<u8>]>,
    head: AtomicUsize, // next byte to read (free-running)
    tail: AtomicUsize, // next byte to write (free-running)
    writer: Mutex<()>,
    dropped: AtomicUsize,
}

// SAFETY: bytes between head and tail belong to the reader, the rest to the
// one writer holding `writer`; the atomics order the hand-over.
unsafe impl Sync for Ring {}
unsafe impl Send for Ring {}

impl Ring {
    pub fn new(size: usize) -> Ring {
        assert!(size.is_power_of_two());
        Ring {
            buf: (0..size).map(|_| UnsafeCell::new(0)).collect(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            writer: Mutex::new(()),
            dropped: AtomicUsize::new(0),
        }
    }

    /// Queues all of `bytes`, or none of them if they don't fit.
    pub fn push(&self, bytes: &[u8]) -> bool {
        let _w = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        let mask = self.buf.len() - 1;
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if self.buf.len() - tail.wrapping_sub(head) < bytes.len() {
            self.dropped.fetch_add(bytes.len(), Ordering::Relaxed);
            return false;
        }
        for (i, &b) in bytes.iter().enumerate() {
            unsafe { *self.buf[(tail.wrapping_add(i)) & mask].get() = b };
        }
        self.tail.store(tail.wrapping_add(bytes.len()), Ordering::Release);
        true
    }

    /// Hands everything queued to `f`, in at most two slices.
    pub fn drain(&self, mut f: impl FnMut(&[u8])) {
        let mask = self.buf.len() - 1;
        let mut head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        let mut scratch = [0u8; 1024];
        while head != tail {
            let n = tail.wrapping_sub(head).min(scratch.len());
            for (i, s) in scratch[..n].iter_mut().enumerate() {
                *s = unsafe { *self.buf[(head.wrapping_add(i)) & mask].get() };
            }
            head = head.wrapping_add(n);
            self.head.store(head, Ordering::Release);
            f(&scratch[..n]);
        }
    }

    #[cfg(test)]
    pub fn dropped(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_order_across_the_wrap() {
        let r = Ring::new(16);
        let mut got = Vec::new();
        for round in 0..10u8 {
            assert!(r.push(&[round, round + 1, round + 2, round + 3, round + 4]));
            r.drain(|b| got.extend_from_slice(b));
        }
        let want: Vec<u8> = (0..10u8).flat_map(|r| [r, r + 1, r + 2, r + 3, r + 4]).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn refuses_what_does_not_fit() {
        let r = Ring::new(8);
        assert!(r.push(&[1; 6]));
        assert!(!r.push(&[2; 3]));
        assert_eq!(r.dropped(), 3);
        let mut n = 0;
        r.drain(|b| n += b.len());
        assert_eq!(n, 6);
        assert!(r.push(&[3; 8]));
    }
}

//! What the player needs from a synthesizer. The emulated MT-32 (src/mt32.rs)
//! is the only one today. Another kind, such as a General MIDI SoundFont
//! synth, would implement the same trait.

pub trait Synth: Send {
    /// Frames per second that `render` produces.
    fn sample_rate(&self) -> u32;

    /// Raw MIDI bytes as they arrive: any mix of short messages, running
    /// status and SysEx, possibly split anywhere.
    fn feed(&mut self, midi: &[u8]);

    /// Fills `out` with interleaved stereo samples (left, right, ...).
    fn render(&mut self, out: &mut [f32]);

    /// The text on the synth's display, if it has one, and whether its
    /// MIDI activity light is on.
    fn display(&mut self) -> Option<(String, bool)>;
}

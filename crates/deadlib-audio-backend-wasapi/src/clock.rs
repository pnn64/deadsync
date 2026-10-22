//! Relate each submitted packet to the device's correlated position/QPC pair.

use super::Audio;

pub(super) struct PlaybackClock {
    clock: Audio::IAudioClock,
    frequency: u64,
    sample_rate_hz: u32,
    // Native stream frames, including startup silence. This counter starts over
    // whenever a client is opened; the mixer's process-wide frame counter does not.
    submitted_frames: u64,
}

#[derive(Clone, Copy)]
pub(super) struct ClockPosition {
    pub position: u64,
    pub qpc_hns: u64,
}

impl PlaybackClock {
    pub fn new(clock: Audio::IAudioClock, sample_rate_hz: u32) -> Result<Self, String> {
        // SAFETY: the service belongs to a live initialized client. Frequency is
        // constant for this stream and uses the same units as GetPosition.
        let frequency = unsafe { clock.GetFrequency() }
            .map_err(|e| format!("failed to query WASAPI clock frequency: {e}"))?;
        if frequency == 0 || sample_rate_hz == 0 {
            return Err("invalid WASAPI clock frequency or sample rate".to_string());
        }
        Ok(Self {
            clock,
            frequency,
            sample_rate_hz,
            submitted_frames: 0,
        })
    }

    pub fn read(&self) -> Result<ClockPosition, String> {
        let mut sample = ClockPosition {
            position: 0,
            qpc_hns: 0,
        };
        // SAFETY: the clock is live and both out pointers are writable locals.
        unsafe {
            self.clock
                .GetPosition(&mut sample.position, Some(&mut sample.qpc_hns))
                .map_err(|e| format!("failed to query WASAPI audio clock position: {e}"))?;
        }
        Ok(sample)
    }

    pub fn next_packet_anchor(&self) -> Result<Option<u64>, String> {
        Ok(packet_anchor_nanos(
            self.read()?,
            self.frequency,
            self.sample_rate_hz,
            self.submitted_frames,
        ))
    }

    pub fn submitted(&mut self, frames: u32) {
        self.submitted_frames = self.submitted_frames.saturating_add(u64::from(frames));
    }
}

fn packet_anchor_nanos(
    sample: ClockPosition,
    frequency: u64,
    sample_rate_hz: u32,
    submitted_frames: u64,
) -> Option<u64> {
    // A zero position before/just after Start has no usable playback epoch yet.
    // Keep priming with silence instead of assigning music a guessed timestamp.
    if sample.position == 0 || sample.qpc_hns == 0 {
        return None;
    }
    let packet_ns = u128::from(submitted_frames) * 1_000_000_000 / u128::from(sample_rate_hz);
    let played_ns = u128::from(sample.position) * 1_000_000_000 / u128::from(frequency);
    let qpc_ns = u128::from(sample.qpc_hns) * 100;
    // Keep sub-microsecond precision even after long system/stream uptimes.
    // Reserve u64::MAX for the callback window's nanoseconds-plus-one encoding.
    Some(
        (qpc_ns + packet_ns)
            .saturating_sub(played_ns)
            .min(u128::from(u64::MAX - 1)) as u64,
    )
}

#[cfg(test)]
#[path = "../tests/clock/mod.rs"]
mod tests;

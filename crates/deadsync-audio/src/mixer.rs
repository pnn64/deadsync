/// Upper bound (in device frames) on how far ahead of the audible write head a
/// scheduled SFX onset may sit before the mixer treats it as stale and drops it.
/// This is a last-resort sanity bound; seek/stop/track-change staleness is
/// handled by the caller's generation guard.
pub const MAX_SCHEDULE_AHEAD_FRAMES: u64 = 192_000;

/// Where a scheduled SFX onset lands relative to the buffer currently being
/// filled. See [`scheduled_onset_decision`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledOnset {
    /// Target frame is implausibly far ahead; drop the entry.
    Drop,
    /// Onset falls in a later buffer; keep pending without mixing.
    Pending,
    /// Mix starting at this sample offset within the current buffer.
    StartAt(usize),
}

/// Decides where a scheduled SFX onset lands within the buffer the mixer is
/// currently filling. `target_stream_frame == 0` means "play immediately".
/// `total_before` is the absolute write-head frame at the start of this buffer;
/// `buf_len` is the buffer length in interleaved samples.
#[inline(always)]
pub fn scheduled_onset_decision(
    target_stream_frame: u64,
    total_before: u64,
    device_channels: usize,
    buf_len: usize,
) -> ScheduledOnset {
    if target_stream_frame == 0 {
        return ScheduledOnset::StartAt(0);
    }
    let frames_until = target_stream_frame.saturating_sub(total_before);
    if frames_until > MAX_SCHEDULE_AHEAD_FRAMES {
        return ScheduledOnset::Drop;
    }
    let offset = (frames_until as usize) * device_channels;
    if offset >= buf_len {
        return ScheduledOnset::Pending;
    }
    ScheduledOnset::StartAt(offset)
}

#[cfg(test)]
mod tests {
    use super::{MAX_SCHEDULE_AHEAD_FRAMES, ScheduledOnset, scheduled_onset_decision};

    #[test]
    fn scheduled_onset_immediate_when_target_zero() {
        assert_eq!(
            scheduled_onset_decision(0, 10_000, 2, 1_024),
            ScheduledOnset::StartAt(0)
        );
    }

    #[test]
    fn scheduled_onset_starts_at_offset_within_buffer() {
        assert_eq!(
            scheduled_onset_decision(10_100, 10_000, 2, 1_024),
            ScheduledOnset::StartAt(200)
        );
    }

    #[test]
    fn scheduled_onset_pending_when_beyond_buffer() {
        assert_eq!(
            scheduled_onset_decision(10_600, 10_000, 2, 1_024),
            ScheduledOnset::Pending
        );
    }

    #[test]
    fn scheduled_onset_drops_when_implausibly_far_ahead() {
        assert_eq!(
            scheduled_onset_decision(MAX_SCHEDULE_AHEAD_FRAMES + 10_001, 10_000, 2, 1_024),
            ScheduledOnset::Drop
        );
    }

    #[test]
    fn scheduled_onset_fires_when_target_already_passed() {
        assert_eq!(
            scheduled_onset_decision(9_000, 10_000, 2, 1_024),
            ScheduledOnset::StartAt(0)
        );
    }
}

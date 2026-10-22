use super::*;
use std::{cell::Cell, rc::Rc};
use windows::core::{Result, implement};

#[implement(Audio::IAudioClock)]
struct MockClock {
    frequency: u64,
    sample: Rc<Cell<ClockPosition>>,
    invalidated: Rc<Cell<bool>>,
}

impl Audio::IAudioClock_Impl for MockClock_Impl {
    fn GetFrequency(&self) -> Result<u64> {
        Ok(self.frequency)
    }

    fn GetPosition(&self, position: *mut u64, qpc: *mut u64) -> Result<()> {
        if self.invalidated.get() {
            return Err(Audio::AUDCLNT_E_DEVICE_INVALIDATED.into());
        }
        let sample = self.sample.get();
        // SAFETY: these tests call through PlaybackClock::read, which supplies
        // writable stack locals for both outputs.
        unsafe {
            *position = sample.position;
            *qpc = sample.qpc_hns;
        }
        Ok(())
    }

    fn GetCharacteristics(&self) -> Result<u32> {
        Ok(0)
    }
}

#[test]
fn startup_silence_and_reopened_streams_use_native_frame_counts() {
    let sample = Rc::new(Cell::new(ClockPosition {
        position: 0,
        qpc_hns: 0,
    }));
    let invalidated = Rc::new(Cell::new(false));
    let make_clock = |frequency, rate| {
        PlaybackClock::new(
            MockClock {
                frequency,
                sample: sample.clone(),
                invalidated: invalidated.clone(),
            }
            .into(),
            rate,
        )
    };
    assert!(make_clock(0, 48_000).is_err());
    assert!(make_clock(48_000, 0).is_err());
    for epoch in [1_000_000_000u64, 9_000_000_000] {
        sample.set(ClockPosition {
            position: 0,
            qpc_hns: epoch / 100,
        });
        let mut clock = make_clock(48_000, 48_000).unwrap();
        assert_eq!(clock.next_packet_anchor().unwrap(), None);
        clock.submitted(480);
        assert_eq!(clock.next_packet_anchor().unwrap(), None);
        clock.submitted(240);
        sample.set(ClockPosition {
            position: 48,
            qpc_hns: (epoch + 1_000_000) / 100,
        });
        // All 720 native frames count, although none have entered the mixer.
        assert_eq!(
            clock.next_packet_anchor().unwrap(),
            Some(epoch + 15_000_000)
        );
        clock.submitted(480);
        assert_eq!(
            clock.next_packet_anchor().unwrap(),
            Some(epoch + 25_000_000)
        );
        invalidated.set(true);
        assert!(clock.read().is_err());
        invalidated.set(false);
    }
}

#[test]
fn packet_time_is_independent_of_launch_phase_and_callback_delay() {
    for rate in [44_100u32, 48_000, 96_000, 192_000] {
        // Shared clocks can report bytes, exclusive clocks can report frames.
        for units_per_frame in [1u64, 4, 8] {
            let frequency = u64::from(rate) * units_per_frame;
            for epoch in [1_000_000_000u64, 1_001_300_000, 8_640_000_001_900_000] {
                let submitted = u64::from(rate) * 3;
                // A packet at stream time 3 s must keep the same timestamp
                // regardless of when the thread happens to query the device.
                for delay_ms in [0, 1, 2, 5, 17] {
                    let elapsed_ms = 2_900 + delay_ms;
                    let sample = ClockPosition {
                        position: frequency * elapsed_ms / 1000,
                        qpc_hns: (epoch + elapsed_ms * 1_000_000) / 100,
                    };
                    let anchor = packet_anchor_nanos(sample, frequency, rate, submitted).unwrap();
                    // 44.1 kHz positions may be quantized by up to one frame.
                    assert!(
                        anchor.abs_diff(epoch + 3_000_000_000) <= 1_000_000_000 / u64::from(rate)
                    );
                }
            }
        }
    }
}

#[test]
fn device_position_is_used_even_when_the_timestamp_does_not_change() {
    let first = ClockPosition {
        position: 48_000,
        qpc_hns: 20_000_000,
    };
    let later = ClockPosition {
        position: first.position + 96,
        ..first
    };
    let a = packet_anchor_nanos(first, 48_000, 48_000, 48_480).unwrap();
    let b = packet_anchor_nanos(later, 48_000, 48_000, 48_480).unwrap();
    assert_eq!(a - b, 2_000_000);
}

#[test]
fn long_streams_preserve_precision_and_saturate_timestamp_encoding() {
    let frames = 48_000 * 60 * 60 * 24 * 100u64;
    let epoch = 1_000_000_000u64;
    let sample = ClockPosition {
        position: frames * 8,
        qpc_hns: (epoch + frames / 48_000 * 1_000_000_000) / 100,
    };
    let anchor = packet_anchor_nanos(sample, 384_000, 48_000, frames + 1).unwrap();
    assert_eq!(anchor, sample.qpc_hns * 100 + 20_833);
    assert_eq!(
        packet_anchor_nanos(
            ClockPosition {
                position: 1,
                qpc_hns: u64::MAX
            },
            1,
            1,
            u64::MAX
        ),
        Some(u64::MAX - 1)
    );
    assert_eq!(
        packet_anchor_nanos(
            ClockPosition {
                position: u64::MAX,
                qpc_hns: 1
            },
            1,
            1,
            0
        ),
        Some(0)
    );
    assert_eq!(
        packet_anchor_nanos(
            ClockPosition {
                position: 48,
                qpc_hns: 0
            },
            48_000,
            48_000,
            480
        ),
        None
    );
}

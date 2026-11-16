//! Callback rendering regression tests.
use deadlib_audio_core::*;
use std::sync::Arc;
#[allow(dead_code)]
#[path = "../../../tests/support/allocations.rs"]
mod allocations;

fn push(stream: &mut AudioStreamHandle, input: &[i16], generation: u64) -> usize {
    let mut accepted = 0;
    while accepted < input.len() {
        let n = stream.writer.try_push(
            &input[accepted..],
            MusicBlockTiming {
                generation,
                music_start_sec: accepted as f64 / stream.writer.channels() as f64 / 48000.0,
                music_sec_per_frame: 1.0 / 48000.0,
            },
        );
        if n == 0 {
            break;
        }
        accepted += n;
    }
    accepted
}

fn callback(anchor: u64) -> CallbackInfo {
    CallbackInfo {
        anchor_nanos: anchor,
        clock: CallbackClockSource::Instant,
    }
}


#[test]
fn unity_render_round_trips_every_pcm_value() {
    for (volume, target) in [(1.0, 1.0), (2.0, 0.5), (0.5, 2.0), (3.0, 1.0 / 3.0)] {
        set_music_target_gain(target);
        snap_music_gain_generation();
        reset_music_stream_clock_state();
        activate_music_track();
        let input: Vec<_> = (i16::MIN..=i16::MAX).collect();
        let (mut stream, handle) = music_transport(48000, 2);
        assert_eq!(
            push(&mut stream, &input, music_map_generation()),
            input.len()
        );
        let controls = Arc::new(MixControls::new());
        controls.set_stream_gain(volume);
        let mut renderer = RenderState::new(handle, controls, 2);
        let mut output = vec![0; input.len()];
        let report = renderer.render(
            OutputBufferMut::I16(&mut output),
            callback(1_000_000),
            std::iter::empty(),
        );
        assert!(!report.output_underrun);
        assert_eq!(output, input);
        stop_music_track();
    }
    reset_music_target_gain();
}

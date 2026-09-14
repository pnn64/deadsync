#![cfg(windows)]
//! Opt-in silent playback against a real endpoint. Exclusive mode briefly owns the device.

use deadlib_audio_backend_wasapi::{WasapiAccessMode, enumerate_output_devices, prepare, start};
use deadlib_audio_core::{
    MixControls, RenderState, get_output_timing_snapshot, load_callback_clock_snapshot_now,
    music_transport, sfx_transport,
};
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires a Windows audio endpoint with exclusive access enabled"]
fn shared_and_exclusive_render_and_release_endpoint() {
    let devices = enumerate_output_devices().expect("enumerate endpoints");
    let device = devices
        .iter()
        .find(|device| device.is_default)
        .expect("default endpoint");
    // Reopen shared after exclusive to verify that shutdown releases the device.
    for mode in [
        WasapiAccessMode::Shared,
        WasapiAccessMode::Exclusive,
        WasapiAccessMode::Shared,
    ] {
        let prep = prepare(Some(device.id.clone()), device.name.clone(), None, mode)
            .expect("prepare output");
        let ready = prep.ready();
        let (_transport, render_handle) =
            music_transport(ready.device_sample_rate, ready.device_channels);
        let renderer = RenderState::new(
            render_handle,
            Arc::new(MixControls::new()),
            ready.device_channels,
        );
        let (_sender, receiver) = sfx_transport(16);
        let stream = start(prep, renderer, receiver).expect("start output");
        // Require progress throughout playback; mere Start success misses the
        // partial-exclusive-packet failure that used to occur on the first event.
        for _ in 0..5 {
            let before = load_callback_clock_snapshot_now(|_, _| None).3;
            thread::sleep(Duration::from_millis(100));
            let after = load_callback_clock_snapshot_now(|_, _| None).3;
            assert!(
                after.total_frames > before.total_frames,
                "render stalled in {mode:?}"
            );
            assert!(
                after.last_nanos > before.last_nanos,
                "clock stalled in {mode:?}"
            );
            if mode == WasapiAccessMode::Exclusive {
                assert_eq!(
                    after.last_callback_frames,
                    u64::from(get_output_timing_snapshot().buffer_frames)
                );
            }
        }
        let timing = get_output_timing_snapshot();
        eprintln!(
            "{mode:?}: {} Hz, {} frames, period {:.3} ms",
            ready.device_sample_rate,
            timing.buffer_frames,
            timing.device_period_ns as f64 / 1_000_000.0
        );
        let stop = Instant::now();
        drop(stream);
        assert!(stop.elapsed() < Duration::from_secs(1), "shutdown stalled");
    }
}

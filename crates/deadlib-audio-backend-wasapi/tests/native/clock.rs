//! Compare the published mixer clock with a fresh device reading on the next
//! callback. Using a later reading also exercises interpolation/extrapolation.
use super::*;
use deadlib_audio_core::{
    MixControls, load_callback_clock_snapshot_now, music_total_frames, music_transport,
    sfx_transport, stream_position_frames_from_window,
};
use std::sync::Arc;

#[test]
#[ignore = "requires an available Windows audio endpoint"]
fn shared_playback_clock_tracks_device_across_restarts() {
    probe_restarts(WasapiAccessMode::Shared, false);
}

#[test]
#[ignore = "requires a Windows audio endpoint with exclusive access enabled"]
fn exclusive_playback_clock_tracks_device_across_restarts() {
    probe_restarts(WasapiAccessMode::Exclusive, false);
}

#[test]
#[ignore = "requires an available Windows audio endpoint"]
fn legacy_playback_clock_tracks_device_across_restarts() {
    probe_restarts(WasapiAccessMode::Shared, true);
}

fn probe_restarts(mode: WasapiAccessMode, legacy: bool) {
    let _thread_policy = boost_current_thread(ThreadRole::AudioRender);
    let _com = ComGuard::new().unwrap();
    for session in 0..2 {
        let prep = prepare(None, "default".into(), None, mode).unwrap();
        let device = open_output_device(None).unwrap();
        // Declare the event first so it outlives the associated client.
        let event = WasapiEvent(
            // SAFETY: the returned unnamed event is owned by this guard.
            unsafe { Threading::CreateEventW(None, false, false, PCWSTR::null()) }.unwrap(),
        );
        let client = if legacy {
            let client = build_audio_client(&device).unwrap();
            initialize_shared(&client, &prep.format).unwrap();
            client
        } else {
            init::initialize_client(&device, &prep).unwrap().client
        };
        let (_transport, render_handle) = music_transport(prep.sample_rate_hz, prep.channels);
        let mut render =
            RenderState::new(render_handle, Arc::new(MixControls::new()), prep.channels);
        let (_sender, mut receiver) = sfx_transport(16);
        // SAFETY: the client is initialized, the event is live, and the services
        // remain on this thread until the started-client guard has stopped it.
        unsafe {
            client.SetEventHandle(event.0).unwrap();
            let renderer = client.GetService::<Audio::IAudioRenderClient>().unwrap();
            let device_clock = client.GetService::<Audio::IAudioClock>().unwrap();
            let frequency = device_clock.GetFrequency().unwrap();
            let mut clock = PlaybackClock::new(device_clock, prep.sample_rate_hz).unwrap();
            let capacity = client.GetBufferSize().unwrap();
            let base = music_total_frames();
            write_frames(
                &mut clock,
                &renderer,
                &mut render,
                &mut receiver,
                &prep,
                capacity,
            )
            .unwrap();
            // The pre-Start packet must be silent and must not advance music.
            assert_eq!(music_total_frames(), base);
            let mut submitted = u64::from(capacity);
            let mut render_origin = None;
            client.Start().unwrap();
            let _started = StartedAudioClient(&client);
            let mut errors = Vec::new();
            for callback in 0..240 {
                assert_eq!(
                    Threading::WaitForSingleObject(event.0, 1000),
                    Foundation::WAIT_OBJECT_0
                );
                // This reading is independent of the pair used to publish the
                // preceding callback's anchor; compare at its correlated QPC.
                let sample = clock.read().unwrap();
                if callback >= 40
                    && let Some(origin) = render_origin
                {
                    let window = load_callback_clock_snapshot_now(|_, _| None).3;
                    let actual = stream_position_frames_from_window(
                        prep.sample_rate_hz,
                        0,
                        sample.qpc_hns * 100,
                        window,
                    )
                    .unwrap();
                    let expected = origin
                        + sample.position as f64 * prep.sample_rate_hz as f64 / frequency as f64;
                    errors.push((actual - expected) * 1000.0 / prep.sample_rate_hz as f64);
                }
                let available = match mode {
                    WasapiAccessMode::Shared => capacity - client.GetCurrentPadding().unwrap(),
                    WasapiAccessMode::Exclusive => capacity,
                };
                let before = music_total_frames();
                write_frames(
                    &mut clock,
                    &renderer,
                    &mut render,
                    &mut receiver,
                    &prep,
                    available,
                )
                .unwrap();
                if music_total_frames() > before && render_origin.is_none() {
                    render_origin = Some(before as f64 - submitted as f64);
                }
                submitted += u64::from(available);
            }
            assert!(errors.len() >= 190, "device clock never became usable");
            errors.sort_by(f64::total_cmp);
            let median = errors[errors.len() / 2];
            let mean = errors.iter().sum::<f64>() / errors.len() as f64;
            let p05 = errors[errors.len() / 20];
            let p95 = errors[errors.len() * 19 / 20];
            eprintln!(
                "WASAPI clock {mode:?} legacy={legacy} session={session}: rate={} buffer={capacity} median={median:.6}ms mean={mean:.6}ms p05={p05:.6}ms p95={p95:.6}ms",
                prep.sample_rate_hz
            );
            assert!(
                median.abs() < 0.1,
                "persistent device-clock bias: {median} ms"
            );
            assert!(
                p05 > -0.5 && p95 < 0.5,
                "unstable playback clock: {p05}..{p95} ms"
            );
        }
    }
}

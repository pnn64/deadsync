//! Small opt-in native checks for initialization paths hidden from the public API.
use super::*;
use windows::core::Interface;

#[test]
#[ignore = "requires an available Windows audio endpoint"]
fn legacy_shared_stream_renders() {
    let _com = ComGuard::new().unwrap();
    let device = open_output_device(None).unwrap();
    // Declare the event first so every client is released before it is closed.
    // SAFETY: an unnamed event borrows no Rust memory and is owned by its guard.
    let event = WasapiEvent(
        unsafe { Threading::CreateEventW(None, false, false, PCWSTR::null()) }.unwrap(),
    );
    let client = build_audio_client(&device).unwrap();
    let format = get_mix_format_bytes(&client).unwrap();
    initialize_shared(&client, &format).unwrap();
    // SAFETY: the client is initialized, event is live, and returned service
    // interfaces remain on this thread and are dropped before the client.
    unsafe {
        client.SetEventHandle(event.0).unwrap();
        let renderer = client.GetService::<Audio::IAudioRenderClient>().unwrap();
        let clock = client.GetService::<Audio::IAudioClock>().unwrap();
        let frames = client.GetBufferSize().unwrap();
        renderer.GetBuffer(frames).unwrap();
        renderer
            .ReleaseBuffer(frames, Audio::AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)
            .unwrap();
        client.Start().unwrap();
        let _started = StartedAudioClient(&client);
        let mut first = 0;
        clock.GetPosition(&mut first, None).unwrap();
        for _ in 0..10 {
            assert_eq!(
                Threading::WaitForSingleObject(event.0, 1000),
                Foundation::WAIT_OBJECT_0
            );
            let available = frames - client.GetCurrentPadding().unwrap();
            if available > 0 {
                renderer.GetBuffer(available).unwrap();
                renderer
                    .ReleaseBuffer(available, Audio::AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)
                    .unwrap();
            }
        }
        let mut last = 0;
        clock.GetPosition(&mut last, None).unwrap();
        assert!(last > first, "legacy shared device clock stalled");
        let default = query_device_periods_hns(&client).unwrap().0;
        eprintln!(
            "Legacy shared: {frames} frames, default period {:.3} ms",
            default as f64 / 10_000.0
        );
    }
}

#[test]
#[ignore = "requires a Windows 10/11 endpoint supporting IAudioClient3"]
fn modern_shared_selects_driver_minimum() {
    let _com = ComGuard::new().unwrap();
    let device = open_output_device(None).unwrap();
    let prep = prepare(None, "default".into(), None, WasapiAccessMode::Shared).unwrap();
    let initialized = init::initialize_client(&device, &prep).unwrap();
    let client3 = initialized.client.cast::<Audio::IAudioClient3>().unwrap();
    let (mut default, mut fundamental, mut min, mut max) = (0, 0, 0, 0);
    // SAFETY: the format lives in prep, and all out pointers are writable locals.
    unsafe {
        client3.GetSharedModeEnginePeriod(
            waveformat(&prep.format),
            &mut default,
            &mut fundamental,
            &mut min,
            &mut max,
        )
    }
    .unwrap();
    let period = initialized
        .period_frames
        .expect("modern initialization fell back");
    assert!(period >= min && period <= max && period.is_multiple_of(fundamental));
    assert!(period - fundamental < min);
    eprintln!(
        "Modern shared: selected {period} frames, minimum {min}, fundamental {fundamental}, default {default}, maximum {max}"
    );
}

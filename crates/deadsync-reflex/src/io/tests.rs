use super::*;
use std::sync::mpsc;

fn fixture(input: impl Fn(PadEvent) + Send + Sync + 'static) -> (Arc<Device>, Arc<Service>) {
    let d = Arc::new(Device {
        token: 1,
        serial: "TEST123".into(),
        uuid: [1; 16],
        player: 0,
        name: "RE:Flex test".into(),
        core: Mutex::new(Detector::new(DeviceConfig {
            baseline: Some([1000; 16]),
            ..Default::default()
        })),
        alive: AtomicBool::new(true),
        test_until: Mutex::new(None),
        light_error: AtomicBool::new(false),
    });
    let s = Arc::new(Service {
        path: PathBuf::new(),
        registry: Mutex::new(Registry {
            config: Config::default(),
            devices: vec![d.clone()],
            diagnostics: Vec::new(),
        }),
        lights: Mutex::new(([LightFrame::default(); 2], Instant::now())),
        stop: AtomicBool::new(false),
        save_failed: AtomicBool::new(false),
        thread: Mutex::new(None),
        input: Arc::new(input),
        system: Arc::new(|_| {}),
    });
    (d, s)
}
fn report(left: [u16; 4]) -> [u8; 64] {
    let mut values = [1000u16; 16];
    values[..4].copy_from_slice(&left);
    let mut bytes = [0; 64];
    for (value, chunk) in values.iter().zip(bytes.as_chunks_mut::<2>().0) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    bytes
}
fn clock(step: u64) -> impl FnMut() -> (Instant, u64) {
    let start = Instant::now();
    let mut ticks = 0;
    move || {
        let ms = ticks * step;
        ticks += 1;
        (start + Duration::from_millis(ms), ms * 1_000_000)
    }
}

#[test]
fn sensor_transitions_are_not_coalesced_and_disconnect_releases_held_buttons() {
    let (tx, rx) = mpsc::channel();
    let (d, s) = fixture(move |event| {
        tx.send(event).unwrap();
    });
    let mut reports = [
        report([1030, 1000, 1000, 1000]),
        report([1025, 1030, 1000, 1000]),
        report([1000; 4]),
        report([1030, 1000, 1000, 1000]),
    ]
    .into_iter();
    read_reports(
        |out| match reports.next() {
            Some(bytes) => {
                out.copy_from_slice(&bytes);
                Ok(64)
            }
            None => Err("unplugged".into()),
        },
        d.clone(),
        s,
        clock(1),
    );
    let events: Vec<_> = rx
        .try_iter()
        .map(|event| match event {
            PadEvent::RawButton {
                id,
                code,
                pressed,
                host_nanos,
                ..
            } => {
                assert_eq!(id, PadId(REFLEX_PAD_ID_BASE as u32));
                assert_eq!(code, PadCode(BUTTON_CODE_BASE));
                (pressed, host_nanos)
            }
            _ => panic!("unexpected event"),
        })
        .collect();
    assert_eq!(
        events,
        [
            (true, 1_000_000),
            (false, 3_000_000),
            (true, 4_000_000),
            (false, 6_000_000)
        ]
    );
    assert!(!d.alive.load(Ordering::Acquire));
    assert_eq!(d.core.lock().unwrap().panels, 0);
}

#[test]
fn silence_and_malformed_reports_release_input_at_the_stale_deadline() {
    for malformed in [false, true] {
        let (tx, rx) = mpsc::channel();
        let (d, s) = fixture(move |event| {
            tx.send(event).unwrap();
        });
        let mut first = true;
        read_reports(
            |out| {
                if first {
                    first = false;
                    out.copy_from_slice(&report([1030; 4]));
                    Ok(64)
                } else if malformed {
                    out.fill(255);
                    Ok(64)
                } else {
                    Ok(0)
                }
            },
            d.clone(),
            s.clone(),
            clock(100),
        );
        let pressed: Vec<_> = rx
            .try_iter()
            .map(|e| match e {
                PadEvent::RawButton { pressed, .. } => pressed,
                _ => panic!("unexpected event"),
            })
            .collect();
        assert_eq!(pressed, [true, false]);
        assert!(
            s.registry
                .lock()
                .unwrap()
                .diagnostics
                .iter()
                .any(|m| m.contains("timed out"))
        );
        assert!(!d.alive.load(Ordering::Acquire));
    }
}

#[test]
fn blocked_light_write_does_not_block_input_and_failure_keeps_input_alive() {
    let (events, rx) = mpsc::channel();
    let (d, s) = fixture(move |event| {
        events.send(event).unwrap();
    });
    let (entered, entry) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let wd = d.clone();
    let ws = s.clone();
    let writer = thread::spawn(move || {
        write_frames(
            |_| {
                entered.send(()).unwrap();
                gate.recv().unwrap();
                Err("output unavailable".into())
            },
            wd,
            ws,
        )
    });
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    // The input path can deliver a report while the output endpoint is blocked.
    let mut first = true;
    read_reports(
        |out| {
            if first {
                first = false;
                out.copy_from_slice(&report([1030; 4]));
                Ok(64)
            } else {
                assert!(matches!(
                    rx.try_recv(),
                    Ok(PadEvent::RawButton { pressed: true, .. })
                ));
                release.send(()).unwrap();
                Err("input test complete".into())
            }
        },
        d.clone(),
        s.clone(),
        clock(1),
    );
    writer.join().unwrap();
    assert!(d.light_error.load(Ordering::Relaxed));
    // Output errors alone do not mark the connection dead.
    d.alive.store(true, Ordering::Release);
    write_frames(|_| Err("output still unavailable".into()), d.clone(), s);
    assert!(d.alive.load(Ordering::Acquire));
}

#[test]
fn full_frames_are_written_and_short_write_stops_the_frame() {
    let mut packets = Vec::new();
    send_frame(
        &mut |bytes| {
            packets.push(bytes.to_vec());
            Ok(bytes.len())
        },
        &[[1, 2, 3]; 4],
        15,
    )
    .unwrap();
    assert_eq!(packets.len(), 16);
    assert!(
        packets
            .iter()
            .enumerate()
            .all(|(i, p)| p[0] == 0 && p[1] == ((i as u8) << 4 | 15))
    );
    let mut count = 0;
    assert!(
        send_frame(
            &mut |_| {
                count += 1;
                Ok(3)
            },
            &[[0; 3]; 4],
            0
        )
        .is_err()
    );
    assert_eq!(count, 1);
}

#[test]
fn stale_render_frames_go_dark_but_input_lights_follow_sensor_state() {
    let (d, s) = fixture(|_| {});
    let now = Instant::now();
    s.lights.lock().unwrap().0[0].chart[0] = true;
    d.core.lock().unwrap().config.lights = LightMode::Chart;
    assert_ne!(colors(&d, &s, now)[0], [0; 3]);
    assert_eq!(colors(&d, &s, now + Duration::from_secs(1)), [[0; 3]; 4]);
    let mut core = d.core.lock().unwrap();
    core.config.lights = LightMode::Input;
    core.sample([1030; 16], now);
    drop(core);
    assert!(
        colors(&d, &s, now + Duration::from_secs(1))
            .iter()
            .all(|c| *c != [0; 3])
    );
}

#[test]
fn disconnected_editor_tokens_cannot_target_a_replacement_device() {
    let (old, s) = fixture(|_| {});
    let (new, _) = fixture(|_| {});
    let mut replacement = Arc::try_unwrap(new).ok().unwrap();
    replacement.token = 2;
    let mut registry = s.registry.lock().unwrap();
    let old_id = PadDeviceId {
        backend: BackendKind::Reflex,
        index: 1,
    };
    assert!(registry.device(old_id).is_some());
    old.alive.store(false, Ordering::Release);
    assert!(registry.device(old_id).is_none());
    registry.devices = vec![Arc::new(replacement)];
    assert!(registry.device(old_id).is_none());
    assert!(
        registry
            .device(PadDeviceId { index: 2, ..old_id })
            .is_some()
    );
}

#[test]
fn editor_exposes_all_sensors_in_delta_units_and_reports_save_failure() {
    let (d, _) = fixture(|_| {});
    let mut values = [1000; 16];
    values[0] = 995;
    values[15] = 1031;
    d.core.lock().unwrap().sample(values, Instant::now());
    let pad = crate::view::pad(&d, false);
    assert_eq!(pad.buttons.len(), 4);
    let sensors: Vec<_> = pad.buttons.iter().flat_map(|b| b.sensors.iter()).collect();
    assert_eq!(sensors.len(), 16);
    assert_eq!(sensors[0].raw_value, 0);
    assert_eq!(sensors[15].raw_value, 31);
    for (i, sensor) in sensors.iter().enumerate() {
        assert_eq!(sensor.firmware_index, i);
        assert_eq!(sensor.baseline, Some(1000));
        assert_eq!(sensor.release_threshold, Some(25));
    }
    assert!(
        crate::view::pad(&d, true)
            .status
            .unwrap()
            .contains("could not be saved")
    );
}

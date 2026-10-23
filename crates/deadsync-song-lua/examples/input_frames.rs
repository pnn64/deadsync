//! Paired production/frozen frame construction; no Lua execution in the timer.
use std::hint::black_box;
#[path = "../tests/perf/input_frame_2354.rs"]
mod before;
#[path = "../src/input.rs"]
mod input;
#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;

#[cfg(windows)]
fn pin() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
    }
    // SAFETY: only this disposable benchmark worker's affinity changes.
    assert_ne!(
        unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
        0
    );
}
#[cfg(not(windows))]
fn pin() {}

fn main() {
    pin();
    let mut rows = Vec::new();
    for index in [
        None,
        Some(0),
        Some(1),
        Some(2),
        Some(usize::MAX - 1),
        Some(usize::MAX),
    ] {
        let event = input::SongLuaInputEvent {
            player: index,
            controller: index,
            button: "Right",
            game_button: "Right",
            kind: input::SongLuaInputType::Repeat,
            device: "InputDevice_Key",
            device_button: "DeviceButton_right",
            level: -0.0,
            z: -1,
            down: true,
            ago: -0.0,
            is_joystick: false,
            is_mouse: false,
        };
        let a = before::Frame::new(&event);
        let b = input::compiled::Frame::new(&event);
        assert_eq!(a.values(&event), b.values(&event));
        for sample in 0..14 {
            let mut expected = None;
            for current in [sample % 2 == 0, sample % 2 != 0] {
                let measure = metrics::Measurement::start();
                let mut sum = 0usize;
                for _ in 0..65536 {
                    let event = black_box(&event);
                    let data = if current {
                        let frame = input::compiled::Frame::new(event);
                        let values = frame.values(event);
                        [values[3], values[4]].map(|v| match v {
                            mlua::program::Datum::Bytes(b) => b.len(),
                            _ => 0,
                        })
                    } else {
                        let frame = before::Frame::new(event);
                        let values = frame.values(event);
                        [values[3], values[4]].map(|v| match v {
                            mlua::program::Datum::Bytes(b) => b.len(),
                            _ => 0,
                        })
                    };
                    sum = sum.wrapping_add(black_box(data[0] + data[1]));
                }
                let mut row = measure.finish();
                if let Some(value) = expected {
                    assert_eq!(value, sum)
                } else {
                    expected = Some(sum)
                }
                if sample == 0 {
                    continue;
                }
                row["workload"] = format!("enum_{index:?}").into();
                row["current"] = current.into();
                row["operations"] = 65536.into();
                row["sample"] = sample.into();
                rows.push(row);
            }
        }
    }
    std::fs::write(
        std::env::args().nth(1).expect("input_frames REPORT"),
        serde_json::to_vec_pretty(&serde_json::json!({"rows":rows,"equal_fields":true})).unwrap(),
    )
    .unwrap();
}

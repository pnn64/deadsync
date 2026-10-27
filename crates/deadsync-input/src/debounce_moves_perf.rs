use super::*;
use crate::keymap::perf_tests::allocations as perf;
use std::cell::{Cell, RefCell};
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[expect(clippy::too_many_arguments, reason = "frozen production reference")]
#[path = "debounce_moves_original.rs"]
mod original;

#[path = "direction_tokens_original.rs"]
mod original_tokens;

thread_local! {
    static LOGS: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

struct TraceLogger;
impl log::Log for TraceLogger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        LOGS.with(|logs| logs.borrow().is_some())
    }
    fn log(&self, record: &log::Record<'_>) {
        LOGS.with(|logs| {
            if let Some(logs) = logs.borrow_mut().as_mut() {
                logs.push(record.args().to_string());
            }
        });
    }
    fn flush(&self) {}
}

#[derive(Clone, Copy)]
enum Operation {
    Edge {
        slot: usize,
        pressed: bool,
        mask: u32,
        source: InputSource,
        stamp: Instant,
        host: u64,
        now: Instant,
        window: Duration,
        panic_clock: bool,
    },
    Drain(Instant, Duration),
    Clear(usize),
    Prepare(usize),
}

macro_rules! replay {
    ($module:ident, $operations:expr, $logging:expr) => {{
        let mut store = $module::DebounceStore::new();
        let mut trace = Vec::new();
        LOGS.with(|logs| *logs.borrow_mut() = $logging.then(Vec::new));
        for operation in $operations {
            let result = match *operation {
                Operation::Edge {
                    slot,
                    pressed,
                    mask,
                    source,
                    stamp,
                    host,
                    now,
                    window,
                    panic_clock,
                } => {
                    let calls = Cell::new(0);
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        $module::debounce_input_edge_in_store_mut(
                            &mut store,
                            slot,
                            mask,
                            source,
                            pressed,
                            stamp,
                            host,
                            $module::DebounceWindows::uniform(window),
                            || {
                                calls.set(calls.get() + 1);
                                assert!(!panic_clock, "clock panic");
                                now
                            },
                        )
                    }));
                    format!("calls={} result={result:?}", calls.get())
                }
                Operation::Drain(now, window) => {
                    let mut edges = Vec::new();
                    while let Some(edge) = $module::next_due_edge(
                        &mut store,
                        now,
                        $module::DebounceWindows::uniform(window),
                    ) {
                        edges.push(edge);
                        assert!(edges.len() <= 1024);
                    }
                    format!("{edges:?}")
                }
                Operation::Clear(cap) => {
                    store.clear_and_reserve(cap);
                    String::new()
                }
                Operation::Prepare(len) => {
                    store.prepare_slots(len);
                    String::new()
                }
            };
            trace.push(format!("{result} {store:?}"));
        }
        let logs = LOGS.with(|logs| logs.borrow_mut().take());
        (trace, logs)
    }};
}

#[test]
fn state_updates_preserve_edges_heap_layout_clock_calls_panics_and_logs() {
    static LOGGER: TraceLogger = TraceLogger;
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        log::set_logger(&LOGGER).unwrap();
        log::set_max_level(log::LevelFilter::Debug);
    });
    let base = Instant::now() + Duration::from_secs(1);
    let mut operations = Vec::new();
    let mut seed = 0x6d5a_8f23_10ac_7e91_u64;
    for i in 0..2048 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let now = base + Duration::from_micros(seed % 80_000);
        let window = [
            Duration::ZERO,
            Duration::from_micros(1),
            Duration::from_millis(20),
        ][i % 3];
        operations.push(match i % 113 {
            0 => Operation::Prepare((seed % 80) as usize),
            1 => Operation::Clear((seed % 128) as usize),
            _ if i.is_multiple_of(4) => Operation::Drain(now, window),
            _ => Operation::Edge {
                slot: (seed % 64) as usize,
                pressed: seed & 0x80 != 0,
                mask: (seed >> 32) as u32,
                source: if seed & 0x100 != 0 {
                    InputSource::Keyboard
                } else {
                    InputSource::Gamepad
                },
                stamp: base + Duration::from_micros(seed >> 32),
                host: if i.is_multiple_of(5) { u64::MAX } else { seed },
                now,
                window,
                panic_clock: i.is_multiple_of(89),
            },
        });
    }
    operations.push(Operation::Drain(
        base + Duration::from_secs(10),
        Duration::from_millis(20),
    ));
    use super as current;
    for logging in [false, true] {
        assert_eq!(
            replay!(current, &operations, logging),
            replay!(original, &operations, logging)
        );
    }
}

fn edge(store: &mut DebounceStore, slot: usize, pressed: bool, now: Instant, window: Duration) {
    black_box(debounce_input_edge_in_store_mut(
        black_box(store),
        slot,
        1 << (slot % 32),
        InputSource::Keyboard,
        pressed,
        now,
        slot as u64,
        DebounceWindows::uniform(window),
        || now,
    ));
}

fn cycle(
    store: &mut DebounceStore,
    times: &[Instant],
    base: Instant,
    window: Duration,
    chatter: bool,
) {
    for (slot, &now) in times.iter().enumerate() {
        edge(store, slot, true, now, window);
    }
    let release = base + Duration::from_millis(1);
    for slot in 0..times.len() {
        edge(store, slot, false, release, window);
    }
    if chatter {
        for slot in (0..times.len()).step_by(3) {
            edge(store, slot, true, base + Duration::from_millis(2), window);
            edge(store, slot, false, base + Duration::from_millis(3), window);
        }
    }
    let windows = DebounceWindows::uniform(window);
    while let Some(edge) = next_due_edge(store, base + Duration::from_millis(50), windows) {
        black_box(edge);
    }
    black_box(next_due_edge(
        store,
        base + Duration::from_millis(80),
        windows,
    ));
}

fn measure(label: &str, mut work: impl FnMut()) {
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, 256, work);
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_debounce_moves() {
    let base = Instant::now();
    for (count, order, chatter, zero) in [
        (8, "reverse", false, false),
        (64, "reverse", false, false),
        (512, "reverse", false, false),
        (64, "ordered", false, false),
        (64, "ties", false, false),
        (64, "reverse", true, false),
        (64, "ordered", false, true),
    ] {
        let times: Vec<_> = (0..count)
            .map(|i| {
                base + Duration::from_micros(match order {
                    "reverse" => count - i,
                    "ordered" => i,
                    _ => 0,
                } as u64)
            })
            .collect();
        let window = if zero {
            Duration::ZERO
        } else {
            Duration::from_millis(20)
        };
        let mut store = DebounceStore::new();
        store.prepare_slots(count);
        cycle(&mut store, &times, base, window, chatter);
        assert!(store.due_slots.is_empty() && store.active_len == 0);
        measure(
            &format!(
                "cycle/{order}_{count}_chatter{}_zero{}",
                usize::from(chatter),
                usize::from(zero)
            ),
            || {
                cycle(
                    black_box(&mut store),
                    black_box(&times),
                    black_box(base),
                    black_box(window),
                    chatter,
                )
            },
        );
    }
    let mut held = DebounceStore::new();
    held.prepare_slots(64);
    for slot in 0..64 {
        edge(&mut held, slot, true, base, Duration::from_millis(20));
    }
    measure("control/repeated_held", || {
        for slot in 0..64 {
            edge(&mut held, slot, true, base, Duration::from_millis(20));
        }
    });
    for slot in 0..64 {
        edge(
            &mut held,
            slot,
            false,
            base + Duration::from_millis(1),
            Duration::from_millis(20),
        );
    }
    measure("control/not_due", || {
        for _ in 0..128 {
            black_box(next_due_edge(
                black_box(&mut held),
                black_box(base),
                DebounceWindows::uniform(Duration::from_millis(20)),
            ));
        }
    });
}

fn direction_map() -> crate::keymap::Keymap {
    use crate::keymap::{InputBinding, Keymap};
    use deadlib_platform::input::{KeyCode, PadDir};
    let dirs = [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right];
    let mut map = Keymap::default();
    for (i, action) in crate::ALL_VIRTUAL_ACTIONS.into_iter().enumerate() {
        map.bind(
            action,
            &[
                InputBinding::Key(KeyCode::KeyA),
                InputBinding::PadDir(dirs[i % 4]),
                InputBinding::PadDir(dirs[(i + 1) % 4]),
                InputBinding::PadDirOn {
                    device: i % 4,
                    dir: dirs[(i + 2) % 4],
                },
            ],
        );
    }
    map
}

#[test]
fn direction_tokens_preserve_owned_text_roundtrips_and_complete_ini_bytes() {
    use crate::bindings::{
        binding_to_token, keymap_ini_lines, parse_binding_token, write_keymap_ini_section,
    };
    use crate::keymap::InputBinding;
    use deadlib_platform::input::{KeyCode, PadDir};
    let mut inputs = vec![
        InputBinding::Key(KeyCode::KeyA),
        InputBinding::Key(KeyCode::F35),
        parse_binding_token("PadCode[0xFF]").unwrap(),
    ];
    for dir in [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right] {
        inputs.push(InputBinding::PadDir(dir));
        for device in [0, 1, 15, usize::MAX] {
            inputs.push(InputBinding::PadDirOn { device, dir });
        }
    }
    let mut power = 1usize;
    while let Some(next) = power.checked_mul(10) {
        for device in [next - 1, next, next + 1] {
            for dir in [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right] {
                inputs.push(InputBinding::PadDirOn { device, dir });
            }
        }
        power = next;
    }
    let mut seed = 0x9b71_4d23_6ce0_a5f8_u64;
    for _ in 0..4096 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        for dir in [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right] {
            inputs.push(InputBinding::PadDirOn {
                device: seed as usize,
                dir,
            });
        }
    }
    for input in inputs {
        let expected = original_tokens::binding_to_token(input);
        let actual = binding_to_token(input);
        assert_eq!(actual, expected);
        assert_eq!(parse_binding_token(&actual), Some(input));
    }
    let mut large = crate::keymap::Keymap::default();
    large.bind(
        crate::ALL_VIRTUAL_ACTIONS[0],
        &[
            InputBinding::PadDirOn {
                device: usize::MAX,
                dir: PadDir::Right,
            },
            InputBinding::PadDirOn {
                device: 10000,
                dir: PadDir::Left,
            },
        ],
    );
    for map in [
        crate::keymap::Keymap::default(),
        crate::bindings::default_keymap(),
        direction_map(),
        large,
    ] {
        assert_eq!(
            keymap_ini_lines(&map),
            original_tokens::keymap_ini_lines(&map)
        );
        let mut actual = String::from("prefix\u{96ea}\n");
        let mut expected = actual.clone();
        for _ in 0..2 {
            write_keymap_ini_section(&mut actual, &map);
            original_tokens::write_keymap_ini_section(&mut expected, &map);
            assert_eq!(actual, expected);
        }
    }
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_input_direction_text() {
    use crate::bindings::{binding_to_token, keymap_ini_lines, write_keymap_ini_section};
    use crate::keymap::InputBinding;
    use deadlib_platform::input::{KeyCode, PadDir};
    let dirs = [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right];
    let directions: Vec<_> = (0..512)
        .map(|i| InputBinding::PadDir(dirs[i % 4]))
        .collect();
    let keys: Vec<_> = (0..512)
        .map(|i| {
            InputBinding::Key(
                [
                    KeyCode::KeyA,
                    KeyCode::Enter,
                    KeyCode::Digit1,
                    KeyCode::ArrowLeft,
                    KeyCode::F35,
                ][i % 5],
            )
        })
        .collect();
    let devices: Vec<_> = (0..512)
        .map(|i| InputBinding::PadDirOn {
            device: [0, 1, 37, usize::MAX][i / 4 % 4],
            dir: dirs[i % 4],
        })
        .collect();
    let gamepads = vec![crate::bindings::parse_binding_token("PadCode[0xFF]").unwrap(); 512];
    for (label, inputs) in [
        ("wildcard_directions", directions),
        ("keys_control", keys),
        ("device_directions", devices),
        ("gamepads_control", gamepads),
    ] {
        measure(&format!("tokens/{label}"), || {
            for &input in black_box(&inputs) {
                black_box(binding_to_token(black_box(input)));
            }
        });
    }
    let map = direction_map();
    measure("write/owned_map", || {
        black_box(keymap_ini_lines(black_box(&map)));
    });
    let mut text = String::with_capacity(32768);
    measure("write/reused_map", || {
        text.clear();
        write_keymap_ini_section(black_box(&mut text), black_box(&map));
        black_box(&text);
    });
}

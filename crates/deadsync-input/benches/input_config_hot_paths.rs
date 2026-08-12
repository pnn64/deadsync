use deadsync_input::{
    ALL_VIRTUAL_ACTIONS, GamepadCodeBinding, InputBinding, Keymap, gamepad_code_binding_to_token,
    gamepad_code_binding_to_token_reference, load_keymap_from_ini_entries,
    load_keymap_from_ini_entries_reference, parse_input_debounce_seconds,
    parse_input_debounce_seconds_reference,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 7;
const PARSE_ITERS: usize = 100_000;
const TOKEN_ITERS: usize = 10_000;
const KEYMAP_ITERS: usize = 256;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    realloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            realloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: all operations delegate to `System` with the caller-provided
// pointer and layout. Relaxed counters are diagnostic only.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied a valid layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.free_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: the pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.realloc_bytes
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    fn calls(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

#[derive(Clone, Copy)]
struct TimeSample {
    ns_per_item: f64,
    cycles_per_item: f64,
    items_per_second: f64,
    checksum: u64,
}

fn measure_time(items: usize, iterations: usize, mut operation: impl FnMut() -> u64) -> TimeSample {
    for _ in 0..(iterations / 20).max(1) {
        black_box(operation());
    }
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(black_box(operation()));
    }
    let elapsed = started.elapsed();
    let count = (items * iterations) as f64;
    let cycles = cycle_start
        .zip(cycle_counter())
        .map_or(f64::NAN, |(start, end)| end.wrapping_sub(start) as f64);
    TimeSample {
        ns_per_item: elapsed.as_secs_f64() * 1.0e9 / count,
        cycles_per_item: cycles / count,
        items_per_second: count / elapsed.as_secs_f64(),
        checksum,
    }
}

fn measure_alloc(mut operation: impl FnMut() -> u64) -> (AllocSnapshot, u64) {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let checksum = black_box(operation());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    (ALLOC.snapshot().delta(before), checksum)
}

fn measure_pair(
    items: usize,
    iterations: usize,
    mut old: impl FnMut() -> u64,
    mut new: impl FnMut() -> u64,
) -> (TimeSample, TimeSample, AllocSnapshot, AllocSnapshot) {
    let mut old_samples = Vec::with_capacity(SAMPLES);
    let mut new_samples = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        let (old_sample, new_sample) = if sample % 2 == 0 {
            (
                measure_time(items, iterations, &mut old),
                measure_time(items, iterations, &mut new),
            )
        } else {
            let new_sample = measure_time(items, iterations, &mut new);
            let old_sample = measure_time(items, iterations, &mut old);
            (old_sample, new_sample)
        };
        assert_eq!(old_sample.checksum, new_sample.checksum);
        old_samples.push(old_sample);
        new_samples.push(new_sample);
    }
    old_samples.sort_by(|left, right| left.ns_per_item.total_cmp(&right.ns_per_item));
    new_samples.sort_by(|left, right| left.ns_per_item.total_cmp(&right.ns_per_item));
    let old_time = old_samples[SAMPLES / 2];
    let new_time = new_samples[SAMPLES / 2];
    let (old_alloc, old_checksum) = measure_alloc(&mut old);
    let (new_alloc, new_checksum) = measure_alloc(&mut new);
    assert_eq!(old_checksum, new_checksum);
    (old_time, new_time, old_alloc, new_alloc)
}

fn print_pair(
    name: &str,
    old: TimeSample,
    new: TimeSample,
    old_alloc: AllocSnapshot,
    new_alloc: AllocSnapshot,
) {
    println!("\n{name}");
    for (label, time, alloc) in [("old", old, old_alloc), ("new", new, new_alloc)] {
        println!(
            "  {label} {:>8.2} ns/item {:>8.2} cycles/item {:>8.2} Mitem/s {:>4} calls {:>9} churn B/op",
            time.ns_per_item,
            time.cycles_per_item,
            time.items_per_second / 1.0e6,
            alloc.calls(),
            alloc.churn_bytes(),
        );
    }
    println!(
        "  change {:+.2}% latency/cycles {:+.2}% throughput {:+.2}% churn bytes",
        percent(new.ns_per_item, old.ns_per_item),
        percent(new.items_per_second, old.items_per_second),
        percent(
            new_alloc.churn_bytes() as f64,
            old_alloc.churn_bytes() as f64
        ),
    );
}

fn percent(new: f64, old: f64) -> f64 {
    if old == 0.0 {
        return 0.0;
    }
    (new / old - 1.0) * 100.0
}

fn debounce_checksum(inputs: &[&str], reference: bool) -> u64 {
    inputs.iter().fold(0u64, |sum, input| {
        let parsed = if reference {
            parse_input_debounce_seconds_reference(black_box(input))
        } else {
            parse_input_debounce_seconds(black_box(input))
        };
        sum.rotate_left(5) ^ parsed.map_or(u32::MAX, f32::to_bits) as u64
    })
}

fn token_checksum(bindings: &[GamepadCodeBinding], reference: bool) -> u64 {
    bindings.iter().fold(0u64, |sum, &binding| {
        let token = if reference {
            gamepad_code_binding_to_token_reference(black_box(binding))
        } else {
            gamepad_code_binding_to_token(black_box(binding))
        };
        token.bytes().fold(sum.rotate_left(5), |sum, byte| {
            sum.rotate_left(3) ^ u64::from(byte)
        })
    })
}

fn binding_checksum(binding: InputBinding) -> u64 {
    match binding {
        InputBinding::Key(code) => code as u64,
        InputBinding::PadDir(dir) => 0x1000 | dir.ix() as u64,
        InputBinding::PadDirOn { device, dir } => {
            0x2000 ^ (device as u64).rotate_left(7) ^ dir.ix() as u64
        }
        InputBinding::GamepadCode(binding) => {
            let uuid = binding.uuid.unwrap_or_default().into_iter().fold(
                binding.code_u32 as u64
                    ^ (binding.device.unwrap_or_default() as u64).rotate_left(11),
                |sum, byte| sum.rotate_left(3) ^ u64::from(byte),
            );
            0x3000 ^ uuid
        }
    }
}

fn keymap_checksum(keymap: Keymap) -> u64 {
    ALL_VIRTUAL_ACTIONS.into_iter().fold(0u64, |sum, action| {
        let mut sum = sum.rotate_left(3) ^ action.ix() as u64;
        let mut index = 0;
        while let Some(binding) = keymap.binding_at(action, index) {
            sum = sum.rotate_left(5) ^ binding_checksum(binding);
            index += 1;
        }
        sum ^ index as u64
    })
}

const KEYMAP_ENTRIES: [(&str, &str); 36] = [
    ("P1_BACK", "KeyCode::Escape"),
    ("P1_DOWN", "KeyCode::ArrowDown,KeyCode::KeyS"),
    ("P1_LEFT", "KeyCode::ArrowLeft,KeyCode::KeyA"),
    ("P1_MENUDOWN", ""),
    ("P1_MENULEFT", ""),
    ("P1_MENURIGHT", ""),
    ("P1_MENUUP", ""),
    ("P1_OPERATOR", "KeyCode::ScrollLock"),
    ("P1_RESTART", ""),
    ("P1_RIGHT", "KeyCode::ArrowRight,KeyCode::KeyD"),
    ("P1_SELECT", "KeyCode::Slash"),
    ("P1_START", "KeyCode::Enter"),
    ("P1_UP", "KeyCode::ArrowUp,KeyCode::KeyW"),
    ("P2_BACK", "KeyCode::Numpad0"),
    ("P2_DOWN", "KeyCode::Numpad2"),
    ("P2_LEFT", "KeyCode::Numpad4"),
    ("P2_MENUDOWN", ""),
    ("P2_MENULEFT", ""),
    ("P2_MENURIGHT", ""),
    ("P2_MENUUP", ""),
    ("P2_OPERATOR", ""),
    ("P2_RESTART", ""),
    ("P2_RIGHT", "KeyCode::Numpad6"),
    ("P2_SELECT", "KeyCode::NumpadDecimal"),
    ("P2_START", "KeyCode::NumpadEnter"),
    ("P2_UP", "KeyCode::Numpad8"),
    ("SYSTEM_FASTFORWARD", "KeyCode::Tab"),
    ("SYSTEM_SLOWDOWN", "KeyCode::Backquote"),
    ("P1_CENTER", "KeyCode::Space"),
    ("P2_CENTER", "KeyCode::Numpad5"),
    ("P1_COIN", "KeyCode::KeyC"),
    ("P2_COIN", "KeyCode::KeyV"),
    ("NOT_AN_ACTION", "KeyCode::KeyN"),
    ("SYSTEM_FASTFORWARD_EXTRA", "KeyCode::KeyF"),
    ("P1_MENU_UP", "KeyCode::KeyI"),
    ("é", "KeyCode::KeyE"),
];

fn main() {
    let debounce_inputs = [
        "20ms",
        " 200MS ",
        "0.05",
        "50",
        "500mS",
        "-25MS",
        "NaNms",
        "infMS",
        "fast",
        "ms",
        "m",
        "",
        "  ",
        "éMS",
        "100milliseconds",
        "0.0001",
    ];
    let (old, new, old_alloc, new_alloc) = measure_pair(
        debounce_inputs.len(),
        PARSE_ITERS,
        || debounce_checksum(black_box(&debounce_inputs), true),
        || debounce_checksum(black_box(&debounce_inputs), false),
    );
    print_pair(
        "1. borrowed debounce-unit suffix",
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let token_bindings = (0..128)
        .map(|index| GamepadCodeBinding {
            code_u32: (index as u32).wrapping_mul(0x9E37_79B9),
            device: Some(index * 1_003),
            uuid: Some(std::array::from_fn(|byte| {
                (index as u8).wrapping_mul(17).wrapping_add(byte as u8)
            })),
        })
        .collect::<Vec<_>>();
    let (old, new, old_alloc, new_alloc) = measure_pair(
        token_bindings.len(),
        TOKEN_ITERS,
        || token_checksum(black_box(&token_bindings), true),
        || token_checksum(black_box(&token_bindings), false),
    );
    print_pair(
        "2. exact-capacity direct-hex gamepad tokens",
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let (old, new, old_alloc, new_alloc) = measure_pair(
        KEYMAP_ENTRIES.len(),
        KEYMAP_ITERS,
        || keymap_checksum(load_keymap_from_ini_entries_reference(Some(KEYMAP_ENTRIES))),
        || keymap_checksum(load_keymap_from_ini_entries(Some(KEYMAP_ENTRIES))),
    );
    print_pair(
        "3. stack-key and bitset keymap load",
        old,
        new,
        old_alloc,
        new_alloc,
    );
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the x86-64 timestamp counter.
    Some(unsafe {
        core::arch::x86_64::_mm_lfence();
        let value = core::arch::x86_64::_rdtsc();
        core::arch::x86_64::_mm_lfence();
        value
    })
}

#[cfg(not(target_arch = "x86_64"))]
fn cycle_counter() -> Option<u64> {
    None
}

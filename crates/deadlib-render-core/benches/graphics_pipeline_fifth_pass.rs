use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP: usize = 2_000;
const SAMPLES: usize = 100;
const OPS_PER_SAMPLE: usize = 250;
const ALLOC_OPS: usize = 5_000;
const COMMAND_OPS: usize = 512;

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

// SAFETY: every operation delegates unchanged to `System`; counters only
// observe successful allocator calls while the benchmark gate is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
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
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
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

    fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct Measurement {
    ns_per_op: f64,
    cycles_per_op: Option<f64>,
    p95_ns: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure_pair(mut old: impl FnMut() -> u64, mut new: impl FnMut() -> u64) -> [Measurement; 2] {
    let mut ops: [&mut dyn FnMut() -> u64; 2] = [&mut old, &mut new];
    for round in 0..WARMUP {
        black_box(ops[round % 2]());
        black_box(ops[(round + 1) % 2]());
    }

    let mut elapsed = [Duration::ZERO; 2];
    let mut cycles = [Some(0u64); 2];
    let mut checksums = [0u64; 2];
    let mut samples: [Vec<Duration>; 2] = std::array::from_fn(|_| Vec::with_capacity(SAMPLES));
    for sample in 0..SAMPLES {
        for offset in 0..2 {
            let index = (sample + offset) % 2;
            let cycle_start = cycle_counter();
            let started = Instant::now();
            let mut checksum = 0u64;
            for _ in 0..OPS_PER_SAMPLE {
                checksum = checksum.wrapping_add(black_box(ops[index]()));
            }
            let sample_elapsed = started.elapsed();
            let cycle_end = cycle_counter();
            elapsed[index] += sample_elapsed;
            samples[index].push(sample_elapsed);
            checksums[index] = checksums[index].wrapping_add(checksum);
            cycles[index] = cycles[index]
                .zip(cycle_start.zip(cycle_end))
                .map(|(total, (start, end))| total.wrapping_add(end.wrapping_sub(start)));
        }
    }

    let allocated: [AllocSnapshot; 2] = std::array::from_fn(|index| {
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        for _ in 0..ALLOC_OPS {
            black_box(ops[index]());
        }
        ALLOC.enabled.store(false, Ordering::Relaxed);
        ALLOC.snapshot().delta(before)
    });
    let operations = (SAMPLES * OPS_PER_SAMPLE) as f64;
    std::array::from_fn(|index| {
        samples[index].sort_unstable();
        Measurement {
            ns_per_op: elapsed[index].as_secs_f64() * 1_000_000_000.0 / operations,
            cycles_per_op: cycles[index].map(|value| value as f64 / operations),
            p95_ns: samples[index][SAMPLES * 95 / 100].as_secs_f64() * 1_000_000_000.0
                / OPS_PER_SAMPLE as f64,
            allocated: allocated[index],
            checksum: checksums[index],
        }
    })
}

#[derive(Clone, Copy)]
enum CommandKind {
    Sprite,
    Mesh,
    TexturedMesh,
}

#[derive(Clone, Copy)]
struct CommandOp {
    kind: CommandKind,
    camera: u8,
    texture: u32,
    blend: u8,
}

#[inline(never)]
fn driver_call(work: u64, call: u64) -> u64 {
    black_box(
        work.rotate_left(7)
            .wrapping_add(call)
            .wrapping_mul(0x9e37_79b9),
    )
}

fn command_stream() -> [CommandOp; COMMAND_OPS] {
    std::array::from_fn(|index| CommandOp {
        kind: match index % 4 {
            0 | 2 => CommandKind::Sprite,
            1 => CommandKind::Mesh,
            _ => CommandKind::TexturedMesh,
        },
        camera: ((index / 32) & 1) as u8,
        texture: ((index / 8) % 4 + 1) as u32,
        blend: ((index / 16) & 1) as u8,
    })
}

fn texture_command_stream() -> [CommandOp; COMMAND_OPS] {
    std::array::from_fn(|index| CommandOp {
        kind: match index % 8 {
            3 => CommandKind::Mesh,
            7 => CommandKind::TexturedMesh,
            _ => CommandKind::Sprite,
        },
        camera: ((index / 32) & 1) as u8,
        texture: ((index / 32) % 4 + 1) as u32,
        blend: (index & 1) as u8,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CommandResult {
    semantic: u64,
    calls: u32,
}

fn semantic_draw(semantic: u64, index: usize, op: CommandOp) -> u64 {
    let kind = match op.kind {
        CommandKind::Sprite => 1u64,
        CommandKind::Mesh => 2,
        CommandKind::TexturedMesh => 3,
    };
    semantic
        .rotate_left(5)
        .wrapping_add(index as u64)
        .wrapping_add(kind << 32)
        .wrapping_add(u64::from(op.texture) << 8)
        .wrapping_add(u64::from(op.camera) << 4)
        .wrapping_add(u64::from(op.blend))
}

fn record_camera_commands(ops: &[CommandOp], retain_compatible: bool) -> CommandResult {
    let mut work = 0u64;
    let mut calls = 0u32;
    let mut semantic = 0u64;
    let mut kind = None;
    let mut camera = None;
    let mut call = |tag| {
        work = driver_call(work, tag);
        calls += 1;
    };
    for (index, &op) in ops.iter().enumerate() {
        let kind_key = match op.kind {
            CommandKind::Sprite => 0u8,
            CommandKind::Mesh => 1,
            CommandKind::TexturedMesh => 2,
        };
        if kind != Some(kind_key) {
            call(1); // pipeline kind
            kind = Some(kind_key);
            if !retain_compatible {
                camera = None;
            }
        }
        if camera != Some(op.camera) {
            call(2); // camera push/bind
            camera = Some(op.camera);
        }
        call(3); // draw
        semantic = semantic_draw(semantic, index, op);
    }
    black_box(work);
    CommandResult { semantic, calls }
}

fn record_texture_commands(ops: &[CommandOp], retain_compatible: bool) -> CommandResult {
    let mut work = 0u64;
    let mut calls = 0u32;
    let mut semantic = 0u64;
    let mut kind = None;
    let mut texture = None;
    let mut blend = None;
    let mut call = |tag| {
        work = driver_call(work, tag);
        calls += 1;
    };
    for (index, &op) in ops.iter().enumerate() {
        let kind_key = match op.kind {
            CommandKind::Sprite => 0u8,
            CommandKind::Mesh => 1,
            CommandKind::TexturedMesh => 2,
        };
        if kind != Some(kind_key) {
            kind = Some(kind_key);
            blend = None;
            if !retain_compatible {
                texture = None;
            }
        }
        if blend != Some(op.blend) {
            call(1); // pipeline
            blend = Some(op.blend);
            if !retain_compatible {
                texture = None;
            }
        }
        if !matches!(op.kind, CommandKind::Mesh) {
            let next = (op.texture, matches!(op.kind, CommandKind::TexturedMesh));
            if texture != Some(next) {
                call(2); // texture bind group or descriptor set
                texture = Some(next);
            }
        }
        call(3); // draw
        semantic = semantic_draw(semantic, index, op);
    }
    black_box(work);
    CommandResult { semantic, calls }
}

fn record_vertex_commands(ops: &[CommandOp], retain_slots: bool) -> CommandResult {
    let mut work = 0u64;
    let mut calls = 0u32;
    let mut semantic = 0u64;
    let mut kind = None;
    let mut instance = None;
    let mut index_bound = false;
    let mut call = |tag| {
        work = driver_call(work, tag);
        calls += 1;
    };
    for (index, &op) in ops.iter().enumerate() {
        let kind_key = match op.kind {
            CommandKind::Sprite => 0u8,
            CommandKind::Mesh => 1,
            CommandKind::TexturedMesh => 2,
        };
        if kind != Some(kind_key) {
            call(1); // pipeline
            match op.kind {
                CommandKind::Sprite => {
                    call(2); // vertex slot 0
                    if !retain_slots || instance != Some(0) {
                        call(3); // vertex slot 1
                        instance = Some(0);
                    }
                    if !retain_slots || !index_bound {
                        call(4); // immutable index buffer
                        index_bound = true;
                    }
                }
                CommandKind::Mesh => call(2), // vertex slot 0
                CommandKind::TexturedMesh => {
                    if !retain_slots || instance != Some(1) {
                        call(3); // vertex slot 1
                        instance = Some(1);
                    }
                }
            }
            kind = Some(kind_key);
        }
        if matches!(op.kind, CommandKind::TexturedMesh) {
            call(2); // geometry in vertex slot 0
        }
        call(5); // draw
        semantic = semantic_draw(semantic, index, op);
    }
    black_box(work);
    CommandResult { semantic, calls }
}

fn print_result(label: &str, result: &Measurement, items: usize) {
    println!(
        "  {label:<29} {:>9.2} ns/op {:>9.2} cycles/op {:>9.2} ns p95 \
         {:>8.2} Mitem/s {:>5.3} alloc {:>5.3} realloc {:>5.3} free {:>9.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.p95_ns,
        items as f64 * 1_000.0 / result.ns_per_op,
        result.allocated.allocs as f64 / ALLOC_OPS as f64,
        result.allocated.reallocs as f64 / ALLOC_OPS as f64,
        result.allocated.frees as f64 / ALLOC_OPS as f64,
        result.allocated.churn_bytes() as f64 / ALLOC_OPS as f64,
    );
}

fn print_change(old: &Measurement, new: &Measurement) {
    println!(
        "  old -> new                    {:>8.2}% latency {:>8.2}% cycles {:>8.2}% p95 {:>8.2}% churn",
        percent_change(old.ns_per_op, new.ns_per_op),
        percent_change(
            old.cycles_per_op.unwrap_or(f64::NAN),
            new.cycles_per_op.unwrap_or(f64::NAN),
        ),
        percent_change(old.p95_ns, new.p95_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 && new == 0.0 {
        return 0.0;
    }
    (new / old - 1.0) * 100.0
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

fn main() {
    let commands = command_stream();
    let old_camera = record_camera_commands(&commands, false);
    let new_camera = record_camera_commands(&commands, true);
    assert_eq!(old_camera.semantic, new_camera.semantic);
    assert!(new_camera.calls < old_camera.calls);
    let [old_camera_result, new_camera_result] = measure_pair(
        || record_camera_commands(&commands, false).semantic,
        || record_camera_commands(&commands, true).semantic,
    );
    assert_eq!(old_camera_result.checksum, new_camera_result.checksum);
    println!(
        "compatible camera state ({COMMAND_OPS} interleaved draws, {} -> {} commands)",
        old_camera.calls, new_camera.calls
    );
    print_result("old: invalidate on kind", &old_camera_result, COMMAND_OPS);
    print_result(
        "new: retain uniform camera",
        &new_camera_result,
        COMMAND_OPS,
    );
    print_change(&old_camera_result, &new_camera_result);

    let texture_commands = texture_command_stream();
    let old_texture = record_texture_commands(&texture_commands, false);
    let new_texture = record_texture_commands(&texture_commands, true);
    assert_eq!(old_texture.semantic, new_texture.semantic);
    assert!(new_texture.calls < old_texture.calls);
    let [old_texture_result, new_texture_result] = measure_pair(
        || record_texture_commands(&texture_commands, false).semantic,
        || record_texture_commands(&texture_commands, true).semantic,
    );
    assert_eq!(old_texture_result.checksum, new_texture_result.checksum);
    println!(
        "\nwgpu texture bindings ({COMMAND_OPS} blend-fragmented draws, {} -> {} commands)",
        old_texture.calls, new_texture.calls
    );
    print_result(
        "old: invalidate on switch",
        &old_texture_result,
        COMMAND_OPS,
    );
    print_result(
        "new: retain exact texture key",
        &new_texture_result,
        COMMAND_OPS,
    );
    print_change(&old_texture_result, &new_texture_result);

    let old_vertex = record_vertex_commands(&commands, false);
    let new_vertex = record_vertex_commands(&commands, true);
    assert_eq!(old_vertex.semantic, new_vertex.semantic);
    assert!(new_vertex.calls < old_vertex.calls);
    let [old_vertex_result, new_vertex_result] = measure_pair(
        || record_vertex_commands(&commands, false).semantic,
        || record_vertex_commands(&commands, true).semantic,
    );
    assert_eq!(old_vertex_result.checksum, new_vertex_result.checksum);
    println!(
        "\nvertex/index binding state ({COMMAND_OPS} interleaved draws, {} -> {} commands)",
        old_vertex.calls, new_vertex.calls
    );
    print_result(
        "old: bind every transition",
        &old_vertex_result,
        COMMAND_OPS,
    );
    print_result(
        "new: retain compatible slots",
        &new_vertex_result,
        COMMAND_OPS,
    );
    print_change(&old_vertex_result, &new_vertex_result);
}

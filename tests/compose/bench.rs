use deadsync::test_support::{compose_case, compose_scenarios};
use deadsync::ui::{actors::Actor, compose};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::error::Error;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    alloc_calls: AtomicU64,
    dealloc_calls: AtomicU64,
    realloc_calls: AtomicU64,
    alloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
    live_bytes: AtomicU64,
    peak_live_bytes: AtomicU64,
    measure_peak_live_bytes: AtomicU64,
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    alloc_calls: u64,
    dealloc_calls: u64,
    realloc_calls: u64,
    alloc_bytes: u64,
    free_bytes: u64,
    live_bytes: u64,
    measure_peak_live_bytes: u64,
}

#[derive(Clone, Copy)]
struct AllocDelta {
    alloc_calls: u64,
    dealloc_calls: u64,
    realloc_calls: u64,
    alloc_bytes: u64,
    free_bytes: u64,
    live_bytes: u64,
    peak_live_delta: u64,
}

struct Args {
    scenario: String,
    case_path: Option<String>,
    iters: u64,
    warmup: u64,
    write_case: Option<String>,
    write_output: Option<String>,
}

struct BenchmarkResult {
    name: String,
    actors: usize,
    objects: usize,
    cameras: usize,
    iters: u64,
    elapsed_s: f64,
    alloc: AllocDelta,
    checksum: u64,
    verification: Option<VerificationResult>,
}

struct VerificationResult {
    expected_hash: String,
    actual_hash: String,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            alloc_calls: AtomicU64::new(0),
            dealloc_calls: AtomicU64::new(0),
            realloc_calls: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
            live_bytes: AtomicU64::new(0),
            peak_live_bytes: AtomicU64::new(0),
            measure_peak_live_bytes: AtomicU64::new(0),
        }
    }

    fn begin_measurement(&self) -> AllocSnapshot {
        let live = self.live_bytes.load(Ordering::Relaxed);
        self.measure_peak_live_bytes.store(live, Ordering::Relaxed);
        self.snapshot()
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            alloc_calls: self.alloc_calls.load(Ordering::Relaxed),
            dealloc_calls: self.dealloc_calls.load(Ordering::Relaxed),
            realloc_calls: self.realloc_calls.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
            live_bytes: self.live_bytes.load(Ordering::Relaxed),
            measure_peak_live_bytes: self.measure_peak_live_bytes.load(Ordering::Relaxed),
        }
    }

    fn note_live(&self, live: u64) {
        update_peak(&self.peak_live_bytes, live);
        update_peak(&self.measure_peak_live_bytes, live);
    }

    fn add_live(&self, size: usize) {
        let live = self.live_bytes.fetch_add(size as u64, Ordering::Relaxed) + size as u64;
        self.note_live(live);
    }

    fn sub_live(&self, size: usize) {
        let _ = self.live_bytes.fetch_sub(size as u64, Ordering::Relaxed);
    }
}

impl AllocSnapshot {
    fn diff(self, start: Self) -> AllocDelta {
        AllocDelta {
            alloc_calls: self.alloc_calls.saturating_sub(start.alloc_calls),
            dealloc_calls: self.dealloc_calls.saturating_sub(start.dealloc_calls),
            realloc_calls: self.realloc_calls.saturating_sub(start.realloc_calls),
            alloc_bytes: self.alloc_bytes.saturating_sub(start.alloc_bytes),
            free_bytes: self.free_bytes.saturating_sub(start.free_bytes),
            live_bytes: self.live_bytes.saturating_sub(start.live_bytes),
            peak_live_delta: self
                .measure_peak_live_bytes
                .saturating_sub(start.live_bytes),
        }
    }
}

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.alloc_calls.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
            self.add_live(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        self.dealloc_calls.fetch_add(1, Ordering::Relaxed);
        self.free_bytes
            .fetch_add(layout.size() as u64, Ordering::Relaxed);
        self.sub_live(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let out = unsafe { System.realloc(ptr, layout, new_size) };
        if !out.is_null() {
            self.realloc_calls.fetch_add(1, Ordering::Relaxed);
            if new_size >= layout.size() {
                let delta = new_size - layout.size();
                self.alloc_bytes.fetch_add(delta as u64, Ordering::Relaxed);
                self.add_live(delta);
            } else {
                let delta = layout.size() - new_size;
                self.free_bytes.fetch_add(delta as u64, Ordering::Relaxed);
                self.sub_live(delta);
            }
        }
        out
    }
}

fn update_peak(slot: &AtomicU64, value: u64) {
    let mut current = slot.load(Ordering::Relaxed);
    while value > current {
        match slot.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    if args.case_path.is_none() && args.write_case.is_some() && args.scenario == "all" {
        return Err("--write-case requires a single --scenario value".into());
    }
    if let Some(case_path) = &args.case_path {
        print_result(run_case(&args, case_path)?);
    } else if args.scenario == "all" {
        for &name in compose_scenarios::scenario_names() {
            print_result(run_named(&args, name)?);
        }
    } else {
        print_result(run_named(&args, &args.scenario)?);
    }
    Ok(())
}

fn run_named(args: &Args, name: &str) -> Result<BenchmarkResult, Box<dyn Error>> {
    let scenario = compose_scenarios::build_scenario(name).ok_or_else(|| {
        format!(
            "unknown scenario '{name}', expected one of: all, {}",
            compose_scenarios::scenario_names().join(", ")
        )
    })?;
    if let Some(path) = &args.write_case {
        let (case, output) = compose_case::capture_case(
            scenario.name,
            &scenario.actors,
            scenario.clear_color,
            &scenario.metrics,
            &scenario.fonts,
            scenario.total_elapsed,
        )?;
        compose_case::write_case(std::path::Path::new(path), &case)?;
        if let Some(output_path) = &args.write_output {
            compose_case::write_render_snapshot(std::path::Path::new(output_path), &output)?;
        }
    }
    Ok(benchmark_compose(
        scenario.name,
        &scenario.actors,
        scenario.clear_color,
        &scenario.metrics,
        &scenario.fonts,
        args.iters,
        args.warmup,
        |idx| scenario.total_elapsed + (idx & 63) as f32 * 0.016,
        None,
    ))
}

fn run_case(args: &Args, case_path: &str) -> Result<BenchmarkResult, Box<dyn Error>> {
    let case = compose_case::read_case(std::path::Path::new(case_path))?;
    let replay = compose_case::replay_case(&case)?;
    let output = compose_case::render_case_output(&case)?;
    let actual_hash = compose_case::render_snapshot_hash(&output)?;
    if let Some(path) = &args.write_output {
        compose_case::write_render_snapshot(std::path::Path::new(path), &output)?;
    }
    if actual_hash != case.expected.output_hash {
        return Err(format!(
            "compose output hash mismatch for '{}': expected {} got {}",
            case_path, case.expected.output_hash, actual_hash
        )
        .into());
    }

    Ok(benchmark_compose(
        &replay.screen,
        &replay.actors,
        replay.clear_color,
        &replay.metrics,
        &replay.fonts,
        args.iters,
        args.warmup,
        |_| replay.total_elapsed,
        Some(VerificationResult {
            expected_hash: case.expected.output_hash,
            actual_hash,
        }),
    ))
}

fn benchmark_compose<F>(
    name: &str,
    actors: &[Actor],
    clear_color: [f32; 4],
    metrics: &deadsync::core::space::Metrics,
    fonts: &HashMap<&'static str, deadsync::ui::font::Font>,
    iters: u64,
    warmup: u64,
    elapsed_for_iter: F,
    verification: Option<VerificationResult>,
) -> BenchmarkResult
where
    F: Fn(u64) -> f32,
{
    let sample = compose::build_screen(actors, clear_color, metrics, fonts, elapsed_for_iter(0));
    let objects = sample.objects.len();
    let cameras = sample.cameras.len();
    black_box(objects ^ cameras);

    for idx in 0..warmup {
        let screen =
            compose::build_screen(actors, clear_color, metrics, fonts, elapsed_for_iter(idx));
        black_box(screen.objects.len());
    }

    let start_alloc = ALLOC.begin_measurement();
    let started = Instant::now();
    let mut checksum = 0u64;
    for idx in 0..iters {
        let screen = black_box(compose::build_screen(
            actors,
            clear_color,
            metrics,
            fonts,
            elapsed_for_iter(idx),
        ));
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(screen.objects.len() as u64)
            .wrapping_add(screen.cameras.len() as u64);
        black_box(checksum);
    }

    BenchmarkResult {
        name: name.to_string(),
        actors: actor_count(actors),
        objects,
        cameras,
        iters,
        elapsed_s: started.elapsed().as_secs_f64(),
        alloc: ALLOC.snapshot().diff(start_alloc),
        checksum,
        verification,
    }
}

fn actor_count(actors: &[Actor]) -> usize {
    actors.iter().map(count_actor).sum()
}

fn count_actor(actor: &Actor) -> usize {
    match actor {
        Actor::Frame { children, .. } | Actor::Camera { children, .. } => 1 + actor_count(children),
        Actor::Shadow { child, .. } => 1 + count_actor(child),
        _ => 1,
    }
}

fn print_result(result: BenchmarkResult) {
    let per_iter_s = if result.iters == 0 {
        0.0
    } else {
        result.elapsed_s / result.iters as f64
    };
    let frames_per_s = if result.elapsed_s > 0.0 {
        result.iters as f64 / result.elapsed_s
    } else {
        0.0
    };
    let objects_per_s = frames_per_s * result.objects as f64;
    let allocs_per_iter = ratio(result.alloc.alloc_calls, result.iters);
    let reallocs_per_iter = ratio(result.alloc.realloc_calls, result.iters);
    let bytes_per_iter = ratio(result.alloc.alloc_bytes, result.iters);

    println!("scenario: {}", result.name);
    if let Some(verification) = &result.verification {
        println!(
            "verify: ok expected_hash={} actual_hash={}",
            verification.expected_hash, verification.actual_hash
        );
    }
    println!(
        "shape: actors={} objects/frame={} cameras/frame={}",
        result.actors, result.objects, result.cameras
    );
    println!(
        "time: iters={} total={:.3}s per_iter={:.3}us frames/s={:.1} objects/s={:.0}",
        result.iters,
        result.elapsed_s,
        per_iter_s * 1_000_000.0,
        frames_per_s,
        objects_per_s
    );
    println!(
        "alloc: allocs/iter={:.3} reallocs/iter={:.3} bytes/iter={:.1} live_delta={} peak_live_delta={}",
        allocs_per_iter,
        reallocs_per_iter,
        bytes_per_iter,
        result.alloc.live_bytes,
        result.alloc.peak_live_delta
    );
    println!(
        "alloc_totals: alloc_calls={} dealloc_calls={} realloc_calls={} alloc_bytes={} free_bytes={}",
        result.alloc.alloc_calls,
        result.alloc.dealloc_calls,
        result.alloc.realloc_calls,
        result.alloc.alloc_bytes,
        result.alloc.free_bytes
    );
    println!("checksum: {}", result.checksum);
    println!();
}

fn ratio(total: u64, iters: u64) -> f64 {
    if iters == 0 {
        0.0
    } else {
        total as f64 / iters as f64
    }
}

fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut scenario = String::from("all");
    let mut case_path = None;
    let mut iters = 5_000u64;
    let mut warmup = 500u64;
    let mut write_case = None;
    let mut write_output = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scenario" => scenario = next_value(&mut args, "--scenario")?,
            "--case" => case_path = Some(next_value(&mut args, "--case")?),
            "--iters" => iters = next_value(&mut args, "--iters")?.parse()?,
            "--warmup" => warmup = next_value(&mut args, "--warmup")?.parse()?,
            "--write-case" => write_case = Some(next_value(&mut args, "--write-case")?),
            "--write-output" => write_output = Some(next_value(&mut args, "--write-output")?),
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown arg '{arg}'").into()),
        }
    }

    Ok(Args {
        scenario,
        case_path,
        iters,
        warmup,
        write_case,
        write_output,
    })
}

fn next_value(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, Box<dyn Error>> {
    args.next()
        .ok_or_else(|| format!("missing value for {flag}").into())
}

fn print_help() {
    println!(
        "compose_bench [--scenario all|hud|text|mask] [--case PATH] [--iters N] [--warmup N] [--write-case PATH] [--write-output PATH]"
    );
}

use deadsync::assets::AssetManager;
use deadsync::engine::gfx::RenderList;
use deadsync::engine::present::{actors::Actor, compose};
use deadsync::test_support::{
    compose_case, compose_scenarios, density_graph_bench, density_graph_life_bench, gameplay_bench,
    gameplay_stats_bench, gameplay_stats_double_bench, gameplay_stats_versus_bench,
    gs_scorebox_bench, heart_bg_bench, init_bench, menu_bench, music_wheel_bench, notefield_bench,
    options_bench, pane_stats_bench, player_options_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::error::Error;
use std::hint::black_box;
use std::path::Path;
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
    cache_mode: CacheMode,
    phase: Phase,
    write_case: Option<String>,
    write_actors_output: Option<String>,
    write_output: Option<String>,
    write_resolved_output: Option<String>,
}

#[derive(Clone, Copy)]
enum CacheMode {
    Fresh,
    Retained,
}

#[derive(Clone, Copy)]
enum Phase {
    Actors,
    Compose,
    Resolve,
    ComposeResolve,
}

struct BenchmarkResult {
    name: String,
    phase: Phase,
    cache_mode: CacheMode,
    actors: usize,
    objects: usize,
    cameras: usize,
    iters: u64,
    elapsed_s: f64,
    alloc: AllocDelta,
    checksum: u64,
    verifications: Vec<VerificationResult>,
}

struct VerificationResult {
    kind: &'static str,
    expected_hash: String,
    actual_hash: String,
}

impl CacheMode {
    fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value {
            "fresh" => Ok(Self::Fresh),
            "retained" => Ok(Self::Retained),
            _ => Err(format!("unknown --cache value '{value}', expected fresh or retained").into()),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Retained => "retained",
        }
    }
}

impl Phase {
    fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value {
            "actors" => Ok(Self::Actors),
            "compose" => Ok(Self::Compose),
            "resolve" => Ok(Self::Resolve),
            "compose-resolve" => Ok(Self::ComposeResolve),
            _ => Err(format!(
                "unknown --phase value '{value}', expected actors, compose, resolve, or compose-resolve"
            )
            .into()),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Actors => "actors",
            Self::Compose => "compose",
            Self::Resolve => "resolve",
            Self::ComposeResolve => "compose-resolve",
        }
    }

    const fn needs_compose_output(self) -> bool {
        matches!(self, Self::Resolve | Self::ComposeResolve)
    }
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
    if args.case_path.is_none()
        && args.scenario == "all"
        && (args.write_case.is_some()
            || args.write_actors_output.is_some()
            || args.write_output.is_some()
            || args.write_resolved_output.is_some())
    {
        return Err("write-output options require a single --scenario value".into());
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
    let capture = if args.phase.needs_compose_output()
        || args.write_case.is_some()
        || args.write_actors_output.is_some()
        || args.write_output.is_some()
        || args.write_resolved_output.is_some()
    {
        Some(capture_scenario(&scenario)?)
    } else {
        None
    };

    if let Some((case, output)) = capture.as_ref() {
        write_requested_outputs(args, case, output)?;
    }

    match args.phase {
        Phase::Actors => match name {
            music_wheel_bench::SCENARIO_NAME => {
                let fixture = music_wheel_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(),
                )
            }
            density_graph_bench::SCENARIO_NAME => {
                let fixture = density_graph_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(),
                )
            }
            density_graph_life_bench::SCENARIO_NAME => {
                let fixture = density_graph_life_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(),
                )
            }
            gameplay_stats_bench::SCENARIO_NAME => {
                let fixture = gameplay_stats_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(),
                )
            }
            gameplay_stats_double_bench::SCENARIO_NAME => {
                let fixture = gameplay_stats_double_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(),
                )
            }
            gameplay_stats_versus_bench::SCENARIO_NAME => {
                let fixture = gameplay_stats_versus_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(),
                )
            }
            gameplay_bench::SCENARIO_NAME => {
                let fixture = gameplay_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(matches!(args.cache_mode, CacheMode::Retained)),
                )
            }
            gs_scorebox_bench::SCENARIO_NAME => {
                let fixture = gs_scorebox_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(),
                )
            }
            heart_bg_bench::SCENARIO_NAME => {
                let fixture = heart_bg_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(),
                )
            }
            init_bench::SCENARIO_NAME => {
                let fixture = init_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(matches!(args.cache_mode, CacheMode::Retained)),
                )
            }
            menu_bench::SCENARIO_NAME => {
                let fixture = menu_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(matches!(args.cache_mode, CacheMode::Retained)),
                )
            }
            notefield_bench::SCENARIO_NAME => {
                let fixture = notefield_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(matches!(args.cache_mode, CacheMode::Retained)),
                )
            }
            options_bench::SCENARIO_NAME => {
                let fixture = options_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(matches!(args.cache_mode, CacheMode::Retained)),
                )
            }
            pane_stats_bench::SCENARIO_NAME => {
                let fixture = pane_stats_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(),
                )
            }
            player_options_bench::SCENARIO_NAME => {
                let fixture = player_options_bench::fixture();
                benchmark_actor_builder(
                    scenario.name,
                    scenario.clear_color,
                    &scenario.metrics,
                    &scenario.fonts,
                    scenario.total_elapsed,
                    args.iters,
                    args.warmup,
                    args.cache_mode,
                    || fixture.build(matches!(args.cache_mode, CacheMode::Retained)),
                )
            }
            _ => Err("actors phase currently only supports --scenario music-wheel, density-graph, density-graph-life, gameplay, gameplay-stats, gameplay-stats-double, gameplay-stats-versus, gs-scorebox, heart-bg, init, menu, notefield, options, pane-stats, or player-options".into()),
        },
        Phase::Compose => benchmark_compose(
            scenario.name,
            &scenario.actors,
            scenario.clear_color,
            &scenario.metrics,
            &scenario.fonts,
            args.iters,
            args.warmup,
            args.cache_mode,
            None,
            |idx| scenario.total_elapsed + (idx & 63) as f32 * 0.016,
        ),
        Phase::Resolve => {
            let (case, output) = capture
                .as_ref()
                .ok_or("resolve phase requires a captured compose output")?;
            let assets = asset_manager_for_bench(case)?;
            let render = compose_case::render_list_runtime(output);
            benchmark_resolve(
                scenario.name,
                actor_count(&scenario.actors),
                args.cache_mode,
                &assets,
                render,
                args.iters,
                args.warmup,
            )
        }
        Phase::ComposeResolve => {
            let (case, _) = capture
                .as_ref()
                .ok_or("compose-resolve phase requires a captured compose output")?;
            let assets = asset_manager_for_bench(case)?;
            benchmark_compose_resolve(
                scenario.name,
                &scenario.actors,
                scenario.clear_color,
                &scenario.metrics,
                &scenario.fonts,
                args.iters,
                args.warmup,
                args.cache_mode,
                &assets,
                |idx| scenario.total_elapsed + (idx & 63) as f32 * 0.016,
            )
        }
    }
}

fn run_case(args: &Args, case_path: &str) -> Result<BenchmarkResult, Box<dyn Error>> {
    if matches!(args.phase, Phase::Actors) {
        return Err(
            "actors phase does not support --case; use --scenario music-wheel, density-graph, density-graph-life, gameplay, gameplay-stats, gs-scorebox, heart-bg, init, menu, notefield, options, or pane-stats".into(),
        );
    }
    let case = compose_case::read_case(Path::new(case_path))?;
    let replay = compose_case::replay_case(&case)?;
    let output = compose_case::render_case_output(&case)?;
    let actual_hash = compose_case::render_snapshot_hash(&output)?;
    if actual_hash != case.expected.output_hash {
        return Err(format!(
            "compose output hash mismatch for '{}': expected {} got {}",
            case_path, case.expected.output_hash, actual_hash
        )
        .into());
    }

    if let Some(path) = &args.write_output {
        compose_case::write_render_snapshot(Path::new(path), &output)?;
    }
    if let Some(path) = &args.write_actors_output {
        compose_case::write_actor_snapshot(
            Path::new(path),
            &compose_case::actor_list_snapshot(&replay.actors),
        )?;
    }
    if let Some(path) = &args.write_resolved_output {
        let _assets = asset_manager_for_bench(&case)?;
        let render = compose_case::render_list_runtime(&output);
        let snapshot = compose_case::texture_resolve_snapshot(&render);
        compose_case::write_texture_resolve_snapshot(Path::new(path), &snapshot)?;
    }

    let compose_verification = VerificationResult {
        kind: "compose",
        expected_hash: case.expected.output_hash.clone(),
        actual_hash,
    };

    match args.phase {
        Phase::Actors => unreachable!("actors phase is rejected before case replay"),
        Phase::Compose => benchmark_compose(
            &replay.screen,
            &replay.actors,
            replay.clear_color,
            &replay.metrics,
            &replay.fonts,
            args.iters,
            args.warmup,
            args.cache_mode,
            Some(compose_verification),
            |_| replay.total_elapsed,
        ),
        Phase::Resolve => {
            let assets = asset_manager_for_bench(&case)?;
            let render = compose_case::render_list_runtime(&output);
            benchmark_resolve(
                &replay.screen,
                actor_count(&replay.actors),
                args.cache_mode,
                &assets,
                render,
                args.iters,
                args.warmup,
            )
        }
        Phase::ComposeResolve => {
            let assets = asset_manager_for_bench(&case)?;
            benchmark_compose_resolve(
                &replay.screen,
                &replay.actors,
                replay.clear_color,
                &replay.metrics,
                &replay.fonts,
                args.iters,
                args.warmup,
                args.cache_mode,
                &assets,
                |_| replay.total_elapsed,
            )
        }
    }
}

fn capture_scenario(
    scenario: &compose_scenarios::ComposeScenario,
) -> Result<(compose_case::ComposeCase, compose_case::RenderListSnapshot), Box<dyn Error>> {
    compose_case::capture_case(
        scenario.name,
        &scenario.actors,
        scenario.clear_color,
        &scenario.metrics,
        &scenario.fonts,
        scenario.total_elapsed,
    )
}

fn asset_manager_for_bench(
    case: &compose_case::ComposeCase,
) -> Result<AssetManager, Box<dyn Error>> {
    if case.screen.ends_with("-ci") {
        compose_case::asset_manager_for_case_lowercase(case)
    } else {
        compose_case::asset_manager_for_case(case)
    }
}

fn write_requested_outputs(
    args: &Args,
    case: &compose_case::ComposeCase,
    output: &compose_case::RenderListSnapshot,
) -> Result<(), Box<dyn Error>> {
    if let Some(path) = &args.write_case {
        compose_case::write_case(Path::new(path), case)?;
    }
    if let Some(path) = &args.write_actors_output {
        compose_case::write_actor_snapshot(
            Path::new(path),
            &compose_case::actor_list_snapshot(&compose_case::replay_case(case)?.actors),
        )?;
    }
    if let Some(path) = &args.write_output {
        compose_case::write_render_snapshot(Path::new(path), output)?;
    }
    if let Some(path) = &args.write_resolved_output {
        let _assets = asset_manager_for_bench(case)?;
        let render = compose_case::render_list_runtime(output);
        let snapshot = compose_case::texture_resolve_snapshot(&render);
        compose_case::write_texture_resolve_snapshot(Path::new(path), &snapshot)?;
    }
    Ok(())
}

#[inline(always)]
fn build_screen_for_mode(
    cache_mode: CacheMode,
    text_cache: &mut compose::TextLayoutCache,
    actors: &[Actor],
    clear_color: [f32; 4],
    metrics: &deadsync::engine::space::Metrics,
    fonts: &HashMap<&'static str, deadsync::engine::present::font::Font>,
    total_elapsed: f32,
) -> RenderList {
    match cache_mode {
        CacheMode::Fresh => {
            compose::build_screen(actors, clear_color, metrics, fonts, total_elapsed)
        }
        CacheMode::Retained => compose::build_screen_cached(
            actors,
            clear_color,
            metrics,
            fonts,
            total_elapsed,
            text_cache,
        ),
    }
}

fn benchmark_actor_builder<F>(
    name: &str,
    clear_color: [f32; 4],
    metrics: &deadsync::engine::space::Metrics,
    fonts: &HashMap<&'static str, deadsync::engine::present::font::Font>,
    total_elapsed: f32,
    iters: u64,
    warmup: u64,
    cache_mode: CacheMode,
    build_actors: F,
) -> Result<BenchmarkResult, Box<dyn Error>>
where
    F: Fn() -> Vec<Actor>,
{
    let sample_actors = build_actors();
    let _assets = compose_case::asset_manager_for_scene(name, &sample_actors, fonts)?;
    let actor_snapshot = compose_case::actor_list_snapshot(&sample_actors);
    let actor_hash = compose_case::actor_snapshot_hash(&actor_snapshot)?;
    let mut text_cache = compose::TextLayoutCache::default();
    let sample_render = build_screen_for_mode(
        cache_mode,
        &mut text_cache,
        &sample_actors,
        clear_color,
        metrics,
        fonts,
        total_elapsed,
    );
    let render_hash =
        compose_case::render_snapshot_hash(&compose_case::render_list_snapshot(&sample_render))?;
    let actors = actor_count(&sample_actors);
    let objects = sample_render.objects.len();
    let cameras = sample_render.cameras.len();
    black_box(actors ^ objects ^ cameras);
    drop(sample_render);
    drop(sample_actors);

    for _ in 0..warmup {
        let actors = build_actors();
        black_box(actor_count(&actors));
    }

    let start_alloc = ALLOC.begin_measurement();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iters {
        let actors = black_box(build_actors());
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(actor_count(&actors) as u64)
            .wrapping_add(actors.len() as u64);
        black_box(checksum);
    }

    let final_actors = build_actors();
    let actual_actor_hash =
        compose_case::actor_snapshot_hash(&compose_case::actor_list_snapshot(&final_actors))?;
    if actual_actor_hash != actor_hash {
        return Err(format!(
            "actor hash mismatch after benchmark: expected {} got {}",
            actor_hash, actual_actor_hash
        )
        .into());
    }
    let mut verify_cache = compose::TextLayoutCache::default();
    let final_render = build_screen_for_mode(
        cache_mode,
        &mut verify_cache,
        &final_actors,
        clear_color,
        metrics,
        fonts,
        total_elapsed,
    );
    let actual_render_hash =
        compose_case::render_snapshot_hash(&compose_case::render_list_snapshot(&final_render))?;
    if actual_render_hash != render_hash {
        return Err(format!(
            "actor compose hash mismatch: expected {} got {}",
            render_hash, actual_render_hash
        )
        .into());
    }

    Ok(BenchmarkResult {
        name: name.to_string(),
        phase: Phase::Actors,
        cache_mode,
        actors,
        objects,
        cameras,
        iters,
        elapsed_s: started.elapsed().as_secs_f64(),
        alloc: ALLOC.snapshot().diff(start_alloc),
        checksum,
        verifications: vec![
            VerificationResult {
                kind: "actors",
                expected_hash: actor_hash,
                actual_hash: actual_actor_hash,
            },
            VerificationResult {
                kind: "compose",
                expected_hash: render_hash,
                actual_hash: actual_render_hash,
            },
        ],
    })
}

fn benchmark_compose<F>(
    name: &str,
    actors: &[Actor],
    clear_color: [f32; 4],
    metrics: &deadsync::engine::space::Metrics,
    fonts: &HashMap<&'static str, deadsync::engine::present::font::Font>,
    iters: u64,
    warmup: u64,
    cache_mode: CacheMode,
    verification: Option<VerificationResult>,
    elapsed_for_iter: F,
) -> Result<BenchmarkResult, Box<dyn Error>>
where
    F: Fn(u64) -> f32,
{
    let _assets = compose_case::asset_manager_for_scene(name, actors, fonts)?;
    let mut text_cache = compose::TextLayoutCache::default();
    let sample = build_screen_for_mode(
        cache_mode,
        &mut text_cache,
        actors,
        clear_color,
        metrics,
        fonts,
        elapsed_for_iter(0),
    );
    let objects = sample.objects.len();
    let cameras = sample.cameras.len();
    black_box(objects ^ cameras);

    for idx in 0..warmup {
        let screen = build_screen_for_mode(
            cache_mode,
            &mut text_cache,
            actors,
            clear_color,
            metrics,
            fonts,
            elapsed_for_iter(idx),
        );
        black_box(screen.objects.len());
    }

    let start_alloc = ALLOC.begin_measurement();
    let started = Instant::now();
    let mut checksum = 0u64;
    for idx in 0..iters {
        let screen = black_box(build_screen_for_mode(
            cache_mode,
            &mut text_cache,
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

    Ok(BenchmarkResult {
        name: name.to_string(),
        phase: Phase::Compose,
        cache_mode,
        actors: actor_count(actors),
        objects,
        cameras,
        iters,
        elapsed_s: started.elapsed().as_secs_f64(),
        alloc: ALLOC.snapshot().diff(start_alloc),
        checksum,
        verifications: verification.into_iter().collect(),
    })
}

fn benchmark_resolve(
    name: &str,
    actors: usize,
    cache_mode: CacheMode,
    _assets: &AssetManager,
    render: RenderList,
    iters: u64,
    warmup: u64,
) -> Result<BenchmarkResult, Box<dyn Error>> {
    let objects = render.objects.len();
    let cameras = render.cameras.len();
    let expected_hash = compose_case::texture_resolve_snapshot_hash(
        &compose_case::texture_resolve_snapshot(&render),
    )?;

    for _ in 0..warmup {
        black_box(texture_handle_checksum(&render));
    }

    let start_alloc = ALLOC.begin_measurement();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iters {
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(texture_handle_checksum(&render))
            .wrapping_add(render.cameras.len() as u64);
        black_box(checksum);
    }
    let elapsed_s = started.elapsed().as_secs_f64();
    let alloc = ALLOC.snapshot().diff(start_alloc);
    let actual_hash = compose_case::texture_resolve_snapshot_hash(
        &compose_case::texture_resolve_snapshot(&render),
    )?;
    if actual_hash != expected_hash {
        return Err(format!(
            "resolved texture hash drifted during benchmark: expected {} got {}",
            expected_hash, actual_hash
        )
        .into());
    }

    Ok(BenchmarkResult {
        name: name.to_string(),
        phase: Phase::Resolve,
        cache_mode,
        actors,
        objects,
        cameras,
        iters,
        elapsed_s,
        alloc,
        checksum,
        verifications: vec![VerificationResult {
            kind: "resolve",
            expected_hash,
            actual_hash,
        }],
    })
}

fn benchmark_compose_resolve<F>(
    name: &str,
    actors: &[Actor],
    clear_color: [f32; 4],
    metrics: &deadsync::engine::space::Metrics,
    fonts: &HashMap<&'static str, deadsync::engine::present::font::Font>,
    iters: u64,
    warmup: u64,
    cache_mode: CacheMode,
    _assets: &AssetManager,
    elapsed_for_iter: F,
) -> Result<BenchmarkResult, Box<dyn Error>>
where
    F: Fn(u64) -> f32,
{
    let mut text_cache = compose::TextLayoutCache::default();
    let sample = build_screen_for_mode(
        cache_mode,
        &mut text_cache,
        actors,
        clear_color,
        metrics,
        fonts,
        elapsed_for_iter(0),
    );
    let objects = sample.objects.len();
    let cameras = sample.cameras.len();
    let expected_hash = compose_case::texture_resolve_snapshot_hash(
        &compose_case::texture_resolve_snapshot(&sample),
    )?;

    for idx in 0..warmup {
        let screen = build_screen_for_mode(
            cache_mode,
            &mut text_cache,
            actors,
            clear_color,
            metrics,
            fonts,
            elapsed_for_iter(idx),
        );
        black_box(texture_handle_checksum(&screen));
    }

    let start_alloc = ALLOC.begin_measurement();
    let started = Instant::now();
    let mut checksum = 0u64;
    for idx in 0..iters {
        let screen = build_screen_for_mode(
            cache_mode,
            &mut text_cache,
            actors,
            clear_color,
            metrics,
            fonts,
            elapsed_for_iter(idx),
        );
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(texture_handle_checksum(&screen))
            .wrapping_add(screen.objects.len() as u64);
        black_box(checksum);
    }
    let elapsed_s = started.elapsed().as_secs_f64();
    let alloc = ALLOC.snapshot().diff(start_alloc);

    let final_screen = build_screen_for_mode(
        cache_mode,
        &mut text_cache,
        actors,
        clear_color,
        metrics,
        fonts,
        elapsed_for_iter(0),
    );
    let actual_hash = compose_case::texture_resolve_snapshot_hash(
        &compose_case::texture_resolve_snapshot(&final_screen),
    )?;
    if actual_hash != expected_hash {
        return Err(format!(
            "compose-resolve hash mismatch: expected {} got {}",
            expected_hash, actual_hash
        )
        .into());
    }

    Ok(BenchmarkResult {
        name: name.to_string(),
        phase: Phase::ComposeResolve,
        cache_mode,
        actors: actor_count(actors),
        objects,
        cameras,
        iters,
        elapsed_s,
        alloc,
        checksum,
        verifications: vec![VerificationResult {
            kind: "resolve",
            expected_hash,
            actual_hash,
        }],
    })
}

fn texture_handle_checksum(render: &RenderList) -> u64 {
    render.objects.iter().fold(0u64, |acc, obj| {
        acc.wrapping_mul(131)
            .wrapping_add(obj.texture_handle)
            .wrapping_add(obj.order as u64)
    })
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
    println!("phase: {}", result.phase.as_str());
    println!("cache: {}", result.cache_mode.as_str());
    for verification in &result.verifications {
        println!(
            "verify: ok kind={} expected_hash={} actual_hash={}",
            verification.kind, verification.expected_hash, verification.actual_hash
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
    let mut cache_mode = CacheMode::Retained;
    let mut phase = Phase::Compose;
    let mut write_case = None;
    let mut write_actors_output = None;
    let mut write_output = None;
    let mut write_resolved_output = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scenario" => scenario = next_value(&mut args, "--scenario")?,
            "--case" => case_path = Some(next_value(&mut args, "--case")?),
            "--iters" => iters = next_value(&mut args, "--iters")?.parse()?,
            "--warmup" => warmup = next_value(&mut args, "--warmup")?.parse()?,
            "--cache" => cache_mode = CacheMode::parse(&next_value(&mut args, "--cache")?)?,
            "--phase" => phase = Phase::parse(&next_value(&mut args, "--phase")?)?,
            "--write-case" => write_case = Some(next_value(&mut args, "--write-case")?),
            "--write-actors-output" => {
                write_actors_output = Some(next_value(&mut args, "--write-actors-output")?)
            }
            "--write-output" => write_output = Some(next_value(&mut args, "--write-output")?),
            "--write-resolved-output" => {
                write_resolved_output = Some(next_value(&mut args, "--write-resolved-output")?)
            }
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
        cache_mode,
        phase,
        write_case,
        write_actors_output,
        write_output,
        write_resolved_output,
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
        "compose_bench [--scenario all|hud|text|text-ci|resolve-ci|mask|heart-bg|init|menu|music-wheel|gameplay|gameplay-stats|gs-scorebox] [--case PATH] [--phase actors|compose|resolve|compose-resolve] [--iters N] [--warmup N] [--cache fresh|retained] [--write-case PATH] [--write-actors-output PATH] [--write-output PATH] [--write-resolved-output PATH]"
    );
}

use deadsync::engine::gfx::draw_prep::{
    self, DrawOp, DrawScratch, PrepareStats, SpriteInstanceRaw, TexturedMeshInstanceRaw,
    TexturedMeshSource,
};
use deadsync::engine::gfx::{BlendMode, MeshMode, MeshVertex, RenderList, TexturedMeshVertex};
use deadsync::engine::present::compose;
use deadsync::test_support::{compose_case, compose_scenarios};
use std::alloc::{GlobalAlloc, Layout, System};
use std::error::Error;
use std::hash::Hasher;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use twox_hash::XxHash64;

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
    flip_texture_keys: bool,
    expect_plan_hash: Option<String>,
    write_plan: Option<String>,
}

struct BenchmarkResult {
    name: String,
    objects: usize,
    cameras: usize,
    sprite_instances: usize,
    tmesh_vertices: usize,
    tmesh_instances: usize,
    ops: usize,
    mesh_ops: usize,
    iters: u64,
    elapsed_s: f64,
    alloc: AllocDelta,
    checksum: u64,
    plan_hash: String,
    verification: Option<VerificationResult>,
}

struct VerificationResult {
    expected_hash: String,
    actual_hash: String,
}

#[derive(serde::Serialize)]
struct PlanSnapshot {
    dynamic_upload_vertices: u64,
    cached_upload_vertices: u64,
    sprite_instances: Vec<SpriteInstanceRaw>,
    mesh_vertices: Vec<MeshVertex>,
    tmesh_vertices: Vec<TexturedMeshVertex>,
    tmesh_instances: Vec<TexturedMeshInstanceRaw>,
    ops: Vec<PlanOpSnapshot>,
}

#[derive(serde::Serialize)]
enum PlanOpSnapshot {
    Sprite {
        instance_start: u32,
        instance_count: u32,
        blend: &'static str,
        texture: u64,
        camera: u8,
    },
    Mesh {
        vertex_start: u32,
        vertex_count: u32,
        mode: &'static str,
        blend: &'static str,
        camera: u8,
    },
    TexturedMesh {
        source: PlanTMeshSourceSnapshot,
        instance_start: u32,
        instance_count: u32,
        mode: &'static str,
        blend: &'static str,
        texture: u64,
        camera: u8,
    },
}

#[derive(serde::Serialize)]
enum PlanTMeshSourceSnapshot {
    Transient {
        vertex_start: u32,
        vertex_count: u32,
        geom_key: u64,
    },
    Cached {
        cache_key: u64,
        vertex_count: u32,
    },
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
                .saturating_sub(start.measure_peak_live_bytes),
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

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.realloc_calls.fetch_add(1, Ordering::Relaxed);
            if new_size >= old.size() {
                let delta = new_size - old.size();
                self.alloc_bytes.fetch_add(delta as u64, Ordering::Relaxed);
                self.add_live(delta);
            } else {
                let delta = old.size() - new_size;
                self.free_bytes.fetch_add(delta as u64, Ordering::Relaxed);
                self.sub_live(delta);
            }
        }
        out
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let single_run = args.case_path.is_some() || args.scenario != "all";
    if !single_run && (args.expect_plan_hash.is_some() || args.write_plan.is_some()) {
        return Err(
            "--expect-plan-hash and --write-plan require a single scenario or --case".into(),
        );
    }

    if let Some(case_path) = &args.case_path {
        print_result(run_case(&args, case_path)?);
        return Ok(());
    }

    if args.scenario == "all" {
        for &name in compose_scenarios::scenario_names() {
            print_result(run_scenario(&args, name)?);
        }
        return Ok(());
    }

    print_result(run_scenario(&args, &args.scenario)?);
    Ok(())
}

fn run_scenario(args: &Args, name: &str) -> Result<BenchmarkResult, Box<dyn Error>> {
    let scenario = compose_scenarios::build_scenario(name).ok_or_else(|| {
        format!(
            "unknown scenario '{name}', expected one of: {}",
            compose_scenarios::scenario_names().join(", ")
        )
    })?;
    let render = compose::build_screen(
        &scenario.actors,
        scenario.clear_color,
        &scenario.metrics,
        &scenario.fonts,
        scenario.total_elapsed,
    );
    benchmark_draw(
        scenario.name,
        &render,
        args.iters,
        args.warmup,
        args.flip_texture_keys,
        args.expect_plan_hash.as_deref(),
        args.write_plan.as_deref(),
        None,
    )
}

fn run_case(args: &Args, case_path: &str) -> Result<BenchmarkResult, Box<dyn Error>> {
    let case = compose_case::read_case(std::path::Path::new(case_path))?;
    let output = compose_case::render_case_output(&case)?;
    let actual_hash = compose_case::render_snapshot_hash(&output)?;
    if actual_hash != case.expected.output_hash {
        return Err(format!(
            "compose output hash mismatch for '{}': expected {} got {}",
            case_path, case.expected.output_hash, actual_hash
        )
        .into());
    }

    let render = compose_case::render_list_runtime(&output);
    benchmark_draw(
        &case.screen,
        &render,
        args.iters,
        args.warmup,
        args.flip_texture_keys,
        args.expect_plan_hash.as_deref(),
        args.write_plan.as_deref(),
        Some(VerificationResult {
            expected_hash: case.expected.output_hash,
            actual_hash,
        }),
    )
}

fn benchmark_draw(
    name: &str,
    render: &RenderList,
    iters: u64,
    warmup: u64,
    _flip_texture_keys: bool,
    expect_plan_hash: Option<&str>,
    write_plan: Option<&str>,
    verification: Option<VerificationResult>,
) -> Result<BenchmarkResult, Box<dyn Error>> {
    let mut render = render.clone();
    ensure_texture_handles(&mut render);
    let initial = build_plan(&render)?;
    let plan_hash = plan_snapshot_hash(&initial.snapshot)?;
    if let Some(expected) = expect_plan_hash
        && expected != plan_hash
    {
        return Err(format!(
            "draw plan hash mismatch for '{}': expected {} got {}",
            name, expected, plan_hash
        )
        .into());
    }
    if let Some(path) = write_plan {
        write_json(std::path::Path::new(path), &initial.snapshot)?;
    }

    let mut scratch = DrawScratch::with_capacity(
        initial.snapshot.sprite_instances.len().max(256),
        initial.snapshot.mesh_vertices.len().max(1024),
        initial.snapshot.tmesh_vertices.len().max(1024),
        initial.snapshot.tmesh_instances.len().max(256),
        initial.snapshot.ops.len().max(64),
    );
    for _ in 0..warmup {
        let stats = draw_prep::prepare(&render, &mut scratch, |_, _| true);
        black_box(checksum_plan(&scratch, stats));
    }

    let start_alloc = ALLOC.begin_measurement();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iters {
        let stats = draw_prep::prepare(black_box(&render), &mut scratch, |_, _| true);
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(checksum_plan(&scratch, stats));
        black_box(checksum);
    }

    let mesh_ops = initial
        .snapshot
        .ops
        .iter()
        .filter(|op| matches!(op, PlanOpSnapshot::Mesh { .. }))
        .count();

    Ok(BenchmarkResult {
        name: name.to_string(),
        objects: render.objects.len(),
        cameras: render.cameras.len(),
        sprite_instances: initial.snapshot.sprite_instances.len(),
        tmesh_vertices: initial.snapshot.tmesh_vertices.len(),
        tmesh_instances: initial.snapshot.tmesh_instances.len(),
        ops: initial.snapshot.ops.len(),
        mesh_ops,
        iters,
        elapsed_s: started.elapsed().as_secs_f64(),
        alloc: ALLOC.snapshot().diff(start_alloc),
        checksum,
        plan_hash,
        verification,
    })
}

struct BuiltPlan {
    snapshot: PlanSnapshot,
}

fn build_plan(render: &RenderList) -> Result<BuiltPlan, Box<dyn Error>> {
    let mut scratch = DrawScratch::with_capacity(256, 1024, 1024, 256, 64);
    let stats = draw_prep::prepare(render, &mut scratch, |_, _| true);
    Ok(BuiltPlan {
        snapshot: plan_snapshot(&scratch, stats),
    })
}

fn plan_snapshot(scratch: &DrawScratch, stats: PrepareStats) -> PlanSnapshot {
    PlanSnapshot {
        dynamic_upload_vertices: stats.dynamic_upload_vertices,
        cached_upload_vertices: stats.cached_upload_vertices,
        sprite_instances: scratch.sprite_instances.clone(),
        mesh_vertices: scratch.mesh_vertices.clone(),
        tmesh_vertices: scratch.tmesh_vertices.clone(),
        tmesh_instances: scratch.tmesh_instances.clone(),
        ops: scratch
            .ops
            .iter()
            .map(|op| match *op {
                DrawOp::Sprite(run) => PlanOpSnapshot::Sprite {
                    instance_start: run.instance_start,
                    instance_count: run.instance_count,
                    blend: blend_name(run.blend),
                    texture: run.texture_handle,
                    camera: run.camera,
                },
                DrawOp::Mesh(run) => PlanOpSnapshot::Mesh {
                    vertex_start: run.vertex_start,
                    vertex_count: run.vertex_count,
                    mode: mesh_mode_name(run.mode),
                    blend: blend_name(run.blend),
                    camera: run.camera,
                },
                DrawOp::TexturedMesh(run) => PlanOpSnapshot::TexturedMesh {
                    source: tmesh_source_snapshot(run.source),
                    instance_start: run.instance_start,
                    instance_count: run.instance_count,
                    mode: mesh_mode_name(run.mode),
                    blend: blend_name(run.blend),
                    texture: run.texture_handle,
                    camera: run.camera,
                },
            })
            .collect(),
    }
}

fn checksum_plan(scratch: &DrawScratch, stats: PrepareStats) -> u64 {
    let mut sum = stats.dynamic_upload_vertices;
    sum = sum
        .wrapping_mul(131)
        .wrapping_add(stats.cached_upload_vertices);
    sum = sum
        .wrapping_mul(131)
        .wrapping_add(scratch.sprite_instances.len() as u64);
    sum = sum
        .wrapping_mul(131)
        .wrapping_add(scratch.mesh_vertices.len() as u64);
    sum = sum
        .wrapping_mul(131)
        .wrapping_add(scratch.tmesh_vertices.len() as u64);
    sum = sum
        .wrapping_mul(131)
        .wrapping_add(scratch.tmesh_instances.len() as u64);
    sum = sum.wrapping_mul(131).wrapping_add(scratch.ops.len() as u64);
    if let Some(first) = scratch.ops.first() {
        sum = sum.wrapping_mul(131).wrapping_add(match *first {
            DrawOp::Sprite(run) => run.texture_handle,
            DrawOp::Mesh(run) => {
                u64::from(run.vertex_start)
                    ^ (u64::from(run.vertex_count) << 32)
                    ^ u64::from(run.camera)
            }
            DrawOp::TexturedMesh(run) => tmesh_source_hash(run.source) ^ run.texture_handle,
        });
    }
    sum
}

fn tmesh_source_snapshot(source: TexturedMeshSource) -> PlanTMeshSourceSnapshot {
    match source {
        TexturedMeshSource::Transient {
            vertex_start,
            vertex_count,
            geom_key,
        } => PlanTMeshSourceSnapshot::Transient {
            vertex_start,
            vertex_count,
            geom_key,
        },
        TexturedMeshSource::Cached {
            cache_key,
            vertex_count,
        } => PlanTMeshSourceSnapshot::Cached {
            cache_key,
            vertex_count,
        },
    }
}

fn tmesh_source_hash(source: TexturedMeshSource) -> u64 {
    match source {
        TexturedMeshSource::Transient {
            vertex_start,
            vertex_count,
            geom_key,
        } => geom_key ^ u64::from(vertex_start) ^ (u64::from(vertex_count) << 32),
        TexturedMeshSource::Cached {
            cache_key,
            vertex_count,
        } => cache_key ^ (u64::from(vertex_count) << 32),
    }
}

fn plan_snapshot_hash(snapshot: &PlanSnapshot) -> Result<String, Box<dyn Error>> {
    let bytes = serde_json::to_vec(snapshot)?;
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(&bytes);
    Ok(format!("{:016x}", hasher.finish()))
}

fn ensure_texture_handles(render: &mut RenderList) {
    let mut next_handle = 1u64;
    for obj in &mut render.objects {
        if obj.texture_handle != deadsync::engine::gfx::INVALID_TEXTURE_HANDLE {
            continue;
        }
        obj.texture_handle = match &obj.object_type {
            deadsync::engine::gfx::ObjectType::Sprite { .. }
            | deadsync::engine::gfx::ObjectType::TexturedMesh { .. } => {
                let handle = next_handle;
                next_handle = next_handle.wrapping_add(1).max(1);
                handle
            }
            deadsync::engine::gfx::ObjectType::Mesh { .. } => {
                deadsync::engine::gfx::INVALID_TEXTURE_HANDLE
            }
        };
    }
}

fn blend_name(blend: BlendMode) -> &'static str {
    match blend {
        BlendMode::Alpha => "alpha",
        BlendMode::Add => "add",
        BlendMode::Multiply => "multiply",
        BlendMode::Subtract => "subtract",
    }
}

fn mesh_mode_name(mode: MeshMode) -> &'static str {
    match mode {
        MeshMode::Triangles => "triangles",
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
    let allocs_per_iter = ratio(result.alloc.alloc_calls, result.iters);
    let reallocs_per_iter = ratio(result.alloc.realloc_calls, result.iters);
    let bytes_per_iter = ratio(result.alloc.alloc_bytes, result.iters);

    println!("scenario: {}", result.name);
    if let Some(verification) = &result.verification {
        println!(
            "compose_verify: ok expected_hash={} actual_hash={}",
            verification.expected_hash, verification.actual_hash
        );
    }
    println!("plan_hash: {}", result.plan_hash);
    println!(
        "shape: objects/frame={} cameras/frame={} sprite_instances/frame={} tmesh_vertices/frame={} tmesh_instances/frame={} ops/frame={} mesh_ops/frame={}",
        result.objects,
        result.cameras,
        result.sprite_instances,
        result.tmesh_vertices,
        result.tmesh_instances,
        result.ops,
        result.mesh_ops
    );
    println!(
        "time: iters={} total={:.3}s per_iter={:.3}us plans/s={:.1}",
        result.iters,
        result.elapsed_s,
        per_iter_s * 1_000_000.0,
        frames_per_s
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
    let mut flip_texture_keys = false;
    let mut expect_plan_hash = None;
    let mut write_plan = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scenario" => scenario = next_value(&mut args, "--scenario")?,
            "--case" => case_path = Some(next_value(&mut args, "--case")?),
            "--iters" => iters = next_value(&mut args, "--iters")?.parse()?,
            "--warmup" => warmup = next_value(&mut args, "--warmup")?.parse()?,
            "--flip-texture-key-case" => flip_texture_keys = true,
            "--expect-plan-hash" => {
                expect_plan_hash = Some(next_value(&mut args, "--expect-plan-hash")?)
            }
            "--write-plan" => write_plan = Some(next_value(&mut args, "--write-plan")?),
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
        flip_texture_keys,
        expect_plan_hash,
        write_plan,
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
        "draw_bench [--scenario all|hud|text|mask] [--case PATH] [--iters N] [--warmup N] [--flip-texture-key-case] [--expect-plan-hash HASH] [--write-plan PATH]"
    );
}

fn write_json<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn update_peak(peak: &AtomicU64, value: u64) {
    let mut current = peak.load(Ordering::Relaxed);
    while value > current {
        match peak.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

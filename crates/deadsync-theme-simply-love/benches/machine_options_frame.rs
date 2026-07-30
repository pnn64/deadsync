use deadlib_present::actors::Actor;
use deadlib_present::font::{Font, Glyph};
use deadsync_assets::AssetManager;
use deadsync_config::prelude::Config;
use deadsync_profile::PlayerOptionsData;
use deadsync_theme::views::{
    AppPathView, AppPathsView, AudioOptionsView, GraphicsOptionsView, NoteskinCatalogView,
    SmxAssignmentView, SmxGifCatalogView,
};
use deadsync_theme_simply_love::screens::options::{
    State, benchmark_select_submenu, benchmark_submenu_count, init, push_actors, update,
};
use deadsync_theme_simply_love::views::{
    OptionsInitView, OptionsPackSyncView, SimplyLoveUpdaterCapabilities, SimplyLoveUpdaterView,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 512;
const MEASURE_FRAMES: usize = 20_000;
const FRAME_SECONDS: f32 = 1.0 / 120.0;

struct CountingAlloc {
    allocs: AtomicU64,
    deallocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates to `System` unchanged; the atomics only
// observe successful allocation activity.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied this layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocs.fetch_add(1, Ordering::Relaxed);
        // SAFETY: this pointer/layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: all arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    deallocs: u64,
    reallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            deallocs: self.deallocs - before.deallocs,
            reallocs: self.reallocs - before.reallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct BenchResult {
    update: PhaseResult,
    render: PhaseResult,
    frame: PhaseResult,
    actors: usize,
    checksum: u64,
}

struct PhaseResult {
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
}

fn main() {
    deadsync_theme_simply_love::i18n::init(deadsync_assets::language::load_for_tests("en"));
    let asset_manager = asset_manager();
    let views = [
        ("Main", None, 50, 0x86c3_62ff_793c_9d00),
        ("System", Some(0), 66, 0xa53a_e904_5ac5_16fb),
        ("Graphics", Some(1), 81, 0x568c_e43f_568c_e43f),
        ("Input", Some(2), 29, 0x7e41_f3d0_81be_0c2f),
        ("InputBackend", Some(3), 56, 0x924e_447a_6db1_bb85),
        ("SmxConfig", Some(4), 57, 0x45b8_3567_45b8_3567),
        ("Lights", Some(5), 43, 0xceac_6b54_ceac_6b54),
        ("OnlineScoring", Some(6), 26, 0x6ecd_073e_6ecd_073e),
        ("NullOrDie", Some(7), 23, 0x2e5c_206d_2e5c_206d),
        ("NullOrDieOptions", Some(8), 69, 0x6bef_a473_6bef_a473),
        ("SyncPacks", Some(9), 31, 0x90e1_c604_6f1e_39fb),
        ("Machine", Some(10), 90, 0x7ce5_6492_831a_9b6d),
        ("Advanced", Some(11), 62, 0x5e44_cef9_a1bb_3106),
        ("Course", Some(12), 45, 0xde36_d069_21c9_2f96),
        ("Gameplay", Some(13), 77, 0x8a25_99bd_8a25_99bd),
        ("Sound", Some(14), 71, 0x04c9_5392_04c9_5392),
        ("SelectMusic", Some(15), 85, 0x4069_0e7c_bf96_f183),
        ("GrooveStats", Some(16), 71, 0xe2f0_7e92_e2f0_7e92),
        ("ArrowCloud", Some(17), 40, 0xb63e_4522_49c1_badd),
        ("ScoreImport", Some(18), 49, 0x76cc_8468_76cc_8468),
        ("Folders", Some(19), 62, 0x2254_e09e_ddab_1f61),
    ];
    assert_eq!(views.len(), benchmark_submenu_count() + 1);
    for (label, submenu_index, expected_actors, expected_checksum) in views {
        let mut state = state();
        if let Some(submenu_index) = submenu_index {
            benchmark_select_submenu(&mut state, submenu_index);
        }
        let result = measure(&mut state, &asset_manager);
        assert_eq!(result.actors, expected_actors, "{label} actor count");
        assert_eq!(result.checksum, expected_checksum, "{label} actor checksum");
        assert_zero_alloc(label, &result);
        print_result(label, &result);
    }
}

fn assert_zero_alloc(label: &str, result: &BenchResult) {
    for (phase, alloc) in [
        ("update", result.update.alloc),
        ("render", result.render.alloc),
        ("frame", result.frame.alloc),
    ] {
        assert_eq!(alloc.allocs, 0, "{label} {phase} allocations");
        assert_eq!(alloc.deallocs, 0, "{label} {phase} deallocations");
        assert_eq!(alloc.reallocs, 0, "{label} {phase} reallocations");
        assert_eq!(alloc.bytes, 0, "{label} {phase} allocated bytes");
    }
}

fn measure(state: &mut State, assets: &AssetManager) -> BenchResult {
    let mut actors = Vec::<Actor>::new();
    let updater = SimplyLoveUpdaterView::default();
    let smx = SmxAssignmentView::default();
    let update = measure_update(state, assets, &smx);
    let (render, _, _) = measure_render(state, assets, &updater, &mut actors);
    run_frames(state, assets, &updater, &smx, &mut actors, WARMUP_FRAMES);

    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let checksum = run_frames(state, assets, &updater, &smx, &mut actors, MEASURE_FRAMES);
    BenchResult {
        update,
        render,
        frame: PhaseResult {
            elapsed: started.elapsed(),
            cycles: read_cycles().saturating_sub(cycles_before),
            alloc: ALLOC.snapshot().delta(before),
        },
        actors: actors.len(),
        checksum,
    }
}

fn measure_update(
    state: &mut State,
    assets: &AssetManager,
    smx: &SmxAssignmentView,
) -> PhaseResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(update(state, FRAME_SECONDS, assets, smx));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    for _ in 0..MEASURE_FRAMES {
        black_box(update(state, FRAME_SECONDS, assets, smx));
    }
    PhaseResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
    }
}

fn measure_render(
    state: &State,
    assets: &AssetManager,
    updater: &SimplyLoveUpdaterView,
    actors: &mut Vec<Actor>,
) -> (PhaseResult, usize, u64) {
    for _ in 0..WARMUP_FRAMES {
        actors.clear();
        push_actors(actors, state, assets, updater, 1.0, Default::default());
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..MEASURE_FRAMES {
        actors.clear();
        push_actors(actors, state, assets, updater, 1.0, Default::default());
        checksum = checksum.rotate_left(7) ^ actor_checksum(black_box(actors));
    }
    (
        PhaseResult {
            elapsed: started.elapsed(),
            cycles: read_cycles().saturating_sub(cycles_before),
            alloc: ALLOC.snapshot().delta(before),
        },
        actors.len(),
        checksum,
    )
}

fn run_frames(
    state: &mut State,
    assets: &AssetManager,
    updater: &SimplyLoveUpdaterView,
    smx: &SmxAssignmentView,
    actors: &mut Vec<Actor>,
    frames: usize,
) -> u64 {
    let mut checksum = 0u64;
    for _ in 0..frames {
        black_box(update(state, FRAME_SECONDS, assets, smx));
        actors.clear();
        push_actors(actors, state, assets, updater, 1.0, Default::default());
        checksum = checksum.rotate_left(7) ^ actor_checksum(black_box(actors));
    }
    checksum
}

fn actor_checksum(actors: &[Actor]) -> u64 {
    actors.iter().fold(actors.len() as u64, |sum, actor| {
        let text_len = match actor {
            Actor::Text { content, .. } => content.len() as u64,
            _ => 0,
        };
        sum.rotate_left(3) ^ text_len
    })
}

fn print_result(label: &str, result: &BenchResult) {
    print_phase(label, "update", &result.update);
    print_phase(label, "render", &result.render);
    print_phase(label, "frame ", &result.frame);
    println!(
        "{label:<12} output actors={} checksum={:016x}",
        result.actors, result.checksum
    );
}

fn print_phase(label: &str, phase: &str, result: &PhaseResult) {
    let frames = MEASURE_FRAMES as f64;
    let seconds = result.elapsed.as_secs_f64();
    println!(
        "{label:<12} {phase} {:>8.1} ns/frame  {:>8.0} cycles/frame  {:>8.0} frames/s  \
         {:>5.1} allocs  {:>5.1} frees  {:>4.1} reallocs  {:>7.2} KiB/frame",
        seconds * 1.0e9 / frames,
        result.cycles as f64 / frames,
        frames / seconds,
        result.alloc.allocs as f64 / frames,
        result.alloc.deallocs as f64 / frames,
        result.alloc.reallocs as f64 / frames,
        result.alloc.bytes as f64 / frames / 1024.0,
    );
}

fn asset_manager() -> AssetManager {
    let mut assets = AssetManager::new();
    for name in ["miso", "wendy", "wendy small", "game chars"] {
        assets.register_font(name, test_font());
    }
    assets
}

fn test_font() -> Font {
    let glyph = Glyph {
        texture_key: Arc::from("test/font.png"),
        stroke_texture_key: None,
        tex_rect: [0.0, 0.0, 8.0, 16.0],
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        size: [8.0, 16.0],
        offset: [0.0, 0.0],
        advance: 8.0,
        advance_i32: 8,
    };
    let mut glyph_map = HashMap::new();
    for ch in 32u8..=126 {
        glyph_map.insert(char::from(ch), glyph.clone());
    }
    let mut ascii_glyphs = Box::new(std::array::from_fn(|_| None));
    for ch in 32u8..=126 {
        ascii_glyphs[ch as usize] = Some(glyph.clone());
    }
    Font {
        glyph_map,
        ascii_glyphs,
        default_glyph: Some(glyph),
        line_spacing: 20,
        height: 16,
        fallback_font_name: None,
        cache_tag: 0,
        chain_key: 0,
        default_stroke_color: [0.0, 0.0, 0.0, 1.0],
        stroke_texture_map: HashMap::new(),
        texture_hints_map: HashMap::new(),
    }
}

fn state() -> State {
    init(OptionsInitView {
        config: Config::default(),
        updater_capabilities: SimplyLoveUpdaterCapabilities::default(),
        app_paths: app_paths(),
        audio: AudioOptionsView::default(),
        graphics: GraphicsOptionsView {
            software_thread_choices: vec![0, 1, 2],
            ..Default::default()
        },
        song_packs: Vec::new(),
        pack_sync: OptionsPackSyncView::default(),
        noteskins: NoteskinCatalogView {
            names: vec!["default".to_owned()],
        },
        machine_player_options: PlayerOptionsData::default(),
        smx_assignment: SmxAssignmentView::default(),
        smx_gifs: SmxGifCatalogView::default(),
        score_import_profiles: Vec::new(),
    })
}

fn app_paths() -> AppPathsView {
    let view = |path: &str| AppPathView {
        path: path.into(),
        display: path.to_owned(),
    };
    AppPathsView {
        data: view("/data"),
        cache: view("/cache"),
        songs: view("/data/songs"),
        courses: view("/data/courses"),
        profiles: view("/data/save/profiles"),
        screenshots: view("/data/save/screenshots"),
        log_file: view("/data/deadsync.log"),
        config_file: view("/data/deadsync.ini"),
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: `_rdtsc` reads the processor timestamp counter and has no memory
    // safety preconditions.
    unsafe { core::arch::x86_64::_rdtsc() }
}

#[cfg(not(target_arch = "x86_64"))]
fn read_cycles() -> u64 {
    0
}

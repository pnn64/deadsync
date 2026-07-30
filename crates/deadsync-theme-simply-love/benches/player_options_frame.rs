use deadlib_present::actors::Actor;
use deadlib_present::font::{Font, Glyph};
use deadsync_assets::AssetManager;
use deadsync_chart::{ChartData, SongData};
use deadsync_profile::{PlayStyle, PlayerSide};
use deadsync_theme::views::{NoteskinCatalogView, SmxGifCatalogView};
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
use deadsync_theme_simply_love::screens::player_options::{
    HeartRateDevicesView, benchmark_select_pane, init_for_gameplay, push_actors, update,
};
use deadsync_theme_simply_love::views::PlayerOptionsInitView;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::path::PathBuf;
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
    let panes = [
        ("Main", 106, 0xd1ee_23b1_d1ee_23b1),
        ("Display", 118, 0x3872_2edf_c78d_d120),
        ("Advanced", 114, 0x5989_c449_5989_c449),
        ("Uncommon", 110, 0xc9d4_a525_362b_5ada),
    ];
    for (pane_index, (label, expected_actors, expected_checksum)) in panes.into_iter().enumerate() {
        let mut state = state();
        benchmark_select_pane(&mut state, pane_index);
        let result = measure(&mut state, &asset_manager);
        assert_zero_alloc(label, "update", &result.update);
        assert_zero_alloc(label, "render", &result.render);
        assert_zero_alloc(label, "frame", &result.frame);
        assert_eq!(
            (result.actors, result.checksum),
            (expected_actors, expected_checksum),
            "{label} actor output changed from the pre-optimization baseline"
        );
        print_result(label, &result);
    }
}

fn assert_zero_alloc(label: &str, phase: &str, result: &PhaseResult) {
    assert_eq!(
        (
            result.alloc.allocs,
            result.alloc.deallocs,
            result.alloc.reallocs,
            result.alloc.bytes,
        ),
        (0, 0, 0, 0),
        "{label} {phase} must not allocate in steady state"
    );
}

fn measure(
    state: &mut deadsync_theme_simply_love::screens::player_options::State,
    assets: &AssetManager,
) -> BenchResult {
    let mut actors = Vec::<Actor>::new();
    let update = measure_update(state, assets);
    let (render, _, _) = measure_render(state, assets, &mut actors);
    run_frames(state, assets, &mut actors, WARMUP_FRAMES);

    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let checksum = run_frames(state, assets, &mut actors, MEASURE_FRAMES);
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
    state: &mut deadsync_theme_simply_love::screens::player_options::State,
    assets: &AssetManager,
) -> PhaseResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(update(state, FRAME_SECONDS, assets));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    for _ in 0..MEASURE_FRAMES {
        black_box(update(state, FRAME_SECONDS, assets));
    }
    PhaseResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
    }
}

fn measure_render(
    state: &deadsync_theme_simply_love::screens::player_options::State,
    assets: &AssetManager,
    actors: &mut Vec<Actor>,
) -> (PhaseResult, usize, u64) {
    for _ in 0..WARMUP_FRAMES {
        actors.clear();
        push_actors(actors, state, assets, Default::default());
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..MEASURE_FRAMES {
        actors.clear();
        push_actors(actors, state, assets, Default::default());
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
    state: &mut deadsync_theme_simply_love::screens::player_options::State,
    assets: &AssetManager,
    actors: &mut Vec<Actor>,
    frames: usize,
) -> u64 {
    let mut checksum = 0u64;
    for _ in 0..frames {
        black_box(update(state, FRAME_SECONDS, assets));
        actors.clear();
        push_actors(actors, state, assets, Default::default());
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
        "{label:<10} output actors={} checksum={:016x}",
        result.actors, result.checksum
    );
}

fn print_phase(label: &str, phase: &str, result: &PhaseResult) {
    let frames = MEASURE_FRAMES as f64;
    let seconds = result.elapsed.as_secs_f64();
    println!(
        "{label:<10} {phase} {:>8.1} ns/frame  {:>8.0} cycles/frame  {:>8.0} frames/s  \
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

fn state() -> deadsync_theme_simply_love::screens::player_options::State {
    init_for_gameplay(
        Arc::new(test_song()),
        [0; 2],
        [0; 2],
        1,
        Screen::SelectMusic,
        None,
        NoteskinCatalogView {
            names: vec!["default".to_owned()],
        },
        SmxGifCatalogView::default(),
        HeartRateDevicesView::default(),
        PlayerOptionsInitView {
            play_style: PlayStyle::Versus,
            player_side: PlayerSide::P1,
            joined: [true, true],
            music_rate: 1.0,
            ..Default::default()
        },
    )
}

fn test_song() -> SongData {
    SongData {
        simfile_path: PathBuf::from("benches/player-options/test.ssc"),
        title: "Benchmark Song".to_owned(),
        subtitle: String::new(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: "Benchmark Artist".to_owned(),
        genre: String::new(),
        banner_path: None,
        background_path: None,
        background_changes: Vec::new(),
        background_layer2_changes: Vec::new(),
        foreground_changes: Vec::new(),
        background_lua_changes: Vec::new(),
        foreground_lua_changes: Vec::new(),
        has_lua: false,
        cdtitle_path: None,
        music_path: None,
        display_bpm: "120".to_owned(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 120.0,
        max_bpm: 120.0,
        normalized_bpms: "120".to_owned(),
        music_length_seconds: 120.0,
        first_second: 0.0,
        total_length_seconds: 120,
        precise_last_second_seconds: 120.0,
        charts: vec![test_chart()],
    }
}

fn test_chart() -> ChartData {
    ChartData {
        chart_type: "dance-single".to_owned(),
        difficulty: "Hard".to_owned(),
        description: String::new(),
        chart_name: String::new(),
        meter: 9,
        step_artist: String::new(),
        music_path: None,
        short_hash: "player-options-bench".to_owned(),
        stats: Default::default(),
        tech_counts: Default::default(),
        mines_nonfake: 0,
        stamina_counts: Default::default(),
        total_streams: 0,
        matrix_rating: 0.0,
        max_nps: 0.0,
        sn_detailed_breakdown: String::new(),
        sn_partial_breakdown: String::new(),
        sn_simple_breakdown: String::new(),
        detailed_breakdown: String::new(),
        partial_breakdown: String::new(),
        simple_breakdown: String::new(),
        total_measures: 0,
        measure_nps_vec: Vec::new(),
        measure_seconds_vec: Vec::new(),
        first_second: 0.0,
        has_note_data: false,
        has_chart_attacks: false,
        possible_grade_points: 0,
        holds_total: 0,
        rolls_total: 0,
        mines_total: 0,
        display_bpm: None,
        min_bpm: 120.0,
        max_bpm: 120.0,
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: LFENCE/RDTSC only serialize and read this thread's timestamp
    // counter; they do not dereference memory.
    unsafe {
        core::arch::x86_64::_mm_lfence();
        let cycles = core::arch::x86_64::_rdtsc();
        core::arch::x86_64::_mm_lfence();
        cycles
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn read_cycles() -> u64 {
    0
}

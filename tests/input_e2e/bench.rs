use deadsync::engine::input::{
    self, InputBinding, Keymap, PadDir, PadEvent, PadId, RawKeyboardEvent, VirtualAction,
};
use deadsync::game::gameplay;
use deadsync::screens::{ScreenAction, gameplay as gameplay_screen};
use deadsync::test_support::notefield_bench;
use std::alloc::{GlobalAlloc, Layout, System};
use std::error::Error;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

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
    scenario: Scenario,
    iters: u64,
    warmup: u64,
    frame_batch: u64,
    delta_time: f32,
    no_replay: bool,
}

#[derive(Clone, Copy)]
enum Scenario {
    Key,
    Pad,
    Mixed,
}

#[derive(Clone, Copy)]
enum InputKind {
    Key,
    Pad,
}

struct BenchmarkResult {
    scenario: &'static str,
    iters: u64,
    warmup: u64,
    frame_batch: u64,
    delta_time: f32,
    replay_capture: bool,
    elapsed_s: f64,
    alloc: AllocDelta,
    checksum: u64,
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

impl Scenario {
    fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value {
            "key" => Ok(Self::Key),
            "pad" => Ok(Self::Pad),
            "mixed" => Ok(Self::Mixed),
            _ => Err(
                format!("unknown --scenario value '{value}', expected key, pad, or mixed").into(),
            ),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Pad => "pad",
            Self::Mixed => "mixed",
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    install_bench_keymap();
    print_result(run_benchmark(&args));
    Ok(())
}

fn run_benchmark(args: &Args) -> BenchmarkResult {
    let mut fixture = notefield_bench::fixture();
    reset_fixture(&mut fixture, args.no_replay);
    run_workload(args, fixture.state_mut(), args.warmup, false);
    reset_fixture(&mut fixture, args.no_replay);
    let start_alloc = ALLOC.begin_measurement();
    let started = Instant::now();
    let checksum = run_workload(args, fixture.state_mut(), args.iters, true);
    let elapsed = started.elapsed();
    let end_alloc = ALLOC.snapshot();
    BenchmarkResult {
        scenario: args.scenario.as_str(),
        iters: args.iters,
        warmup: args.warmup,
        frame_batch: args.frame_batch,
        delta_time: args.delta_time,
        replay_capture: !args.no_replay,
        elapsed_s: elapsed.as_secs_f64(),
        alloc: end_alloc.diff(start_alloc),
        checksum,
    }
}

fn reset_fixture(fixture: &mut notefield_bench::NotefieldBenchFixture, no_replay: bool) {
    prepare_gameplay_state(fixture.state_mut());
    gameplay::set_replay_capture_enabled(fixture.state_mut(), !no_replay);
    input::clear_debounce_state();
}

fn run_workload(
    args: &Args,
    state: &mut gameplay::State,
    total_events: u64,
    measured: bool,
) -> u64 {
    let mut checksum = 0u64;
    let base = Instant::now();
    let base_host_nanos = 1_000_000_000u64;
    let dt_host_nanos = 1_000_000u64;
    let dt = Duration::from_nanos(dt_host_nanos);
    let mut i = 0u64;
    while i < total_events {
        let kind = scenario_kind(args.scenario, i);
        let pressed = scenario_pressed(args.scenario, i);
        let timestamp = base + dt.saturating_mul(i as u32);
        let host_nanos = base_host_nanos + dt_host_nanos.saturating_mul(i);
        checksum = match kind {
            InputKind::Key => run_key_event(state, timestamp, host_nanos, pressed, checksum),
            InputKind::Pad => run_pad_event(state, timestamp, host_nanos, pressed, checksum),
        };
        i += 1;
        if i % args.frame_batch == 0 {
            checksum = step_gameplay(state, args.delta_time, checksum, measured);
        }
    }
    if total_events % args.frame_batch != 0 {
        checksum = step_gameplay(state, args.delta_time, checksum, measured);
    }
    checksum
}

#[inline(always)]
fn scenario_kind(scenario: Scenario, event_ix: u64) -> InputKind {
    match scenario {
        Scenario::Key => InputKind::Key,
        Scenario::Pad => InputKind::Pad,
        Scenario::Mixed => [InputKind::Key, InputKind::Pad][(event_ix & 1) as usize],
    }
}

#[inline(always)]
fn scenario_pressed(scenario: Scenario, event_ix: u64) -> bool {
    match scenario {
        Scenario::Key | Scenario::Pad => event_ix & 1 == 0,
        // Mixed alternates sources every event, so each source only gets every
        // other edge. Advance the press/release phase per source pair so both
        // keyboard and pad see a real down/up sequence instead of one source
        // only seeing presses and the other only seeing releases.
        Scenario::Mixed => (event_ix >> 1) & 1 == 0,
    }
}

fn run_key_event(
    state: &mut gameplay::State,
    timestamp: Instant,
    host_nanos: u64,
    pressed: bool,
    mut checksum: u64,
) -> u64 {
    let ev = RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed,
        repeat: false,
        timestamp,
        host_nanos,
    };
    input::map_raw_key_event_with(black_box(&ev), |iev| {
        let action = gameplay_screen::handle_input(state, black_box(&iev));
        checksum = mix_checksum(checksum, checksum_input_event(iev, action));
    });
    checksum
}

fn run_pad_event(
    state: &mut gameplay::State,
    timestamp: Instant,
    host_nanos: u64,
    pressed: bool,
    mut checksum: u64,
) -> u64 {
    let ev = PadEvent::Dir {
        id: PadId(1),
        timestamp,
        host_nanos,
        dir: PadDir::Left,
        pressed,
    };
    input::map_pad_event_with(black_box(&ev), |iev| {
        let action = gameplay_screen::handle_input(state, black_box(&iev));
        checksum = mix_checksum(checksum, checksum_input_event(iev, action));
    });
    checksum
}

fn step_gameplay(
    state: &mut gameplay::State,
    delta_time: f32,
    checksum: u64,
    measured: bool,
) -> u64 {
    let action = gameplay_screen::update(state, delta_time);
    let mut checksum = mix_checksum(checksum, checksum_state(state, action));
    if measured {
        checksum = mix_checksum(
            checksum,
            black_box(state.current_music_time_display.to_bits() as u64),
        );
    }
    checksum
}

fn prepare_gameplay_state(state: &mut gameplay::State) {
    state.autoplay_enabled = false;
    state.song_completed_naturally = false;
    state.exit_transition = None;
    state.hold_to_exit_key = None;
    state.hold_to_exit_start = None;
    state.hold_to_exit_aborted_at = None;
    state.total_elapsed_in_screen = 0.0;
    state.current_beat = 0.0;
    state.current_music_time = 0.0;
    state.current_beat_display = 0.0;
    state.current_music_time_display = 0.0;
    state.current_beat_visible.fill(0.0);
    state.current_music_time_visible.fill(0.0);
    state.current_background_path = None;
    state.next_background_change_ix = 0;
    state.background_texture_key.clear();
    state.notes.clear();
    state.note_ranges.fill((0, 0));
    state.note_spawn_cursor.fill(0);
    state.judged_row_cursor.fill(0);
    state.next_tap_miss_cursor.fill(0);
    state.next_mine_avoid_cursor.fill(0);
    state.next_mine_ix_cursor.fill(0);
    state.row_entries.clear();
    for row_map_cache in &mut state.row_map_cache {
        row_map_cache.clear();
    }
    state.tap_row_hold_roll_flags.clear();
    state.note_time_cache.clear();
    state.note_display_beat_cache.clear();
    state.hold_end_time_cache.clear();
    state.hold_end_display_beat_cache.clear();
    state.notes_end_time = 3_600.0;
    state.music_end_time = 3_600.0;
    state.decaying_hold_indices.clear();
    state.hold_decay_active.clear();
    state.replay_edges.clear();
    for arrows in &mut state.arrows {
        arrows.clear();
    }
    for cues in &mut state.column_cues {
        cues.clear();
    }
    for segments in &mut state.measure_counter_segments {
        segments.clear();
    }
    for segments in &mut state.mini_indicator_stream_segments {
        segments.clear();
    }
    for mine_ix in &mut state.mine_note_ix {
        mine_ix.clear();
    }
    for mine_time in &mut state.mine_note_time {
        mine_time.clear();
    }
    for hold in &mut state.active_holds {
        *hold = None;
    }
    for explosion in &mut state.tap_explosions {
        *explosion = None;
    }
    for explosion in &mut state.mine_explosions {
        *explosion = None;
    }
}

fn install_bench_keymap() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_left,
        &[
            InputBinding::Key(KeyCode::ArrowLeft),
            InputBinding::PadDir(PadDir::Left),
        ],
    );
    input::set_keymap(km);
    input::set_input_debounce_seconds(0.0);
    input::set_only_dedicated_menu_buttons(false);
    input::clear_debounce_state();
}

#[inline(always)]
fn checksum_input_event(ev: input::InputEvent, action: ScreenAction) -> u64 {
    (ev.action.ix() as u64)
        ^ ((ev.pressed as u64) << 8)
        ^ ((matches!(ev.source, input::InputSource::Gamepad) as u64) << 16)
        ^ ev.timestamp_host_nanos.rotate_left(21)
        ^ screen_action_hash(action).rotate_left(7)
}

#[inline(always)]
fn checksum_state(state: &gameplay::State, action: ScreenAction) -> u64 {
    (state.total_elapsed_in_screen.to_bits() as u64)
        ^ (state.current_music_time_display.to_bits() as u64).rotate_left(13)
        ^ (state.players[0].combo as u64).rotate_left(29)
        ^ (state.players[0].life.to_bits() as u64).rotate_left(41)
        ^ screen_action_hash(action)
}

#[inline(always)]
fn screen_action_hash(action: ScreenAction) -> u64 {
    match action {
        ScreenAction::None => 0,
        ScreenAction::Navigate(screen) => 0x1000 | screen as u64,
        ScreenAction::NavigateNoFade(screen) => 0x2000 | screen as u64,
        ScreenAction::Exit => 0x3000,
        ScreenAction::SelectProfiles { .. } => 0x4000,
        ScreenAction::RequestBanner(_) => 0x5000,
        ScreenAction::RequestCdTitle(_) => 0x6000,
        ScreenAction::RequestDensityGraph { .. } => 0x7000,
        ScreenAction::ApplySongOffsetSync { .. } => 0x8000,
        ScreenAction::ApplySongOffsetSyncBatch { .. } => 0x8800,
        ScreenAction::FetchOnlineGrade(_) => 0x9000,
        ScreenAction::ChangeGraphics { .. } => 0xA000,
        ScreenAction::UpdateShowOverlay(mode) => 0xB000 | u64::from(mode),
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

fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut scenario = Scenario::Mixed;
    let mut iters = 200_000u64;
    let mut warmup = 10_000u64;
    let mut frame_batch = 8u64;
    let mut delta_time = 1.0 / 240.0;
    let mut no_replay = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scenario" => {
                scenario = Scenario::parse(&args.next().ok_or("--scenario requires a value")?)?;
            }
            "--iters" => {
                iters = args
                    .next()
                    .ok_or("--iters requires a value")?
                    .parse::<u64>()?;
            }
            "--warmup" => {
                warmup = args
                    .next()
                    .ok_or("--warmup requires a value")?
                    .parse::<u64>()?;
            }
            "--frame-batch" => {
                frame_batch = args
                    .next()
                    .ok_or("--frame-batch requires a value")?
                    .parse::<u64>()?;
            }
            "--delta-time" => {
                delta_time = args
                    .next()
                    .ok_or("--delta-time requires a value")?
                    .parse::<f32>()?;
            }
            "--no-replay" => no_replay = true,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument '{other}'").into()),
        }
    }
    Ok(Args {
        scenario,
        iters,
        warmup,
        frame_batch: frame_batch.max(1),
        delta_time,
        no_replay,
    })
}

fn print_help() {
    println!(
        "usage: cargo run --release --bin input_e2e_bench -- [--scenario key|pad|mixed] [--iters N] [--warmup N] [--frame-batch N] [--delta-time S] [--no-replay]"
    );
}

fn print_result(result: BenchmarkResult) {
    let per_iter_us = if result.iters == 0 {
        0.0
    } else {
        result.elapsed_s * 1_000_000.0 / result.iters as f64
    };
    let events_per_s = if result.elapsed_s > 0.0 {
        result.iters as f64 / result.elapsed_s
    } else {
        0.0
    };
    let allocs_per_iter = ratio(result.alloc.alloc_calls, result.iters);
    let bytes_per_iter = ratio(result.alloc.alloc_bytes, result.iters);

    println!("scenario: {}", result.scenario);
    println!(
        "shape: warmup={} iters={} frame_batch={} delta_time={:.6} replay_capture={}",
        result.warmup, result.iters, result.frame_batch, result.delta_time, result.replay_capture
    );
    println!(
        "time: total={:.6}s per_event={:.3}us events/s={:.0}",
        result.elapsed_s, per_iter_us, events_per_s
    );
    println!(
        "alloc: allocs/event={:.3} bytes/event={:.1} live_delta={} peak_live_delta={}",
        allocs_per_iter, bytes_per_iter, result.alloc.live_bytes, result.alloc.peak_live_delta
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

#[inline(always)]
fn mix_checksum(state: u64, value: u64) -> u64 {
    state
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(value ^ 0x517C_C1B7_2722_0A95)
}

fn ratio(total: u64, iters: u64) -> f64 {
    if iters == 0 {
        0.0
    } else {
        total as f64 / iters as f64
    }
}

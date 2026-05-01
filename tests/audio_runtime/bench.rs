use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::VecDeque;
use std::hint::black_box;
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const CALLBACK_SAMPLES: usize = 1024;
const MUSIC_VOL: f32 = 0.82;
const SFX_VOL: f32 = 0.65;
const MIX_ITERS: usize = 90_000;
const MIX_SFX_ITERS: usize = 35_000;
const QUEUE_MESSAGES: usize = 100_000;
const ACTIVE_GROWTH_ITERS: usize = 100_000;
const POSITION_ITERS: usize = 1_000_000;
const CLOCK_ITERS: usize = 2_000_000;
const SYSCALL_ITERS: usize = 700_000;

#[derive(Clone)]
struct QueuedSfx {
    data: Arc<[i16]>,
    lane: u8,
}

#[derive(Clone, Copy, Default)]
struct MusicMapSeg {
    stream_frame_start: i64,
    frames: i64,
    music_start_sec: f64,
    music_sec_per_frame: f64,
}

#[derive(Default)]
struct PlaybackPosMap {
    queue: VecDeque<MusicMapSeg>,
    backlog_frames: i64,
}

struct CountingAlloc {
    alloc_calls: AtomicU64,
    dealloc_calls: AtomicU64,
    realloc_calls: AtomicU64,
    alloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
    live_bytes: AtomicU64,
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

struct BenchResult {
    name: String,
    iters: usize,
    elapsed: Duration,
    alloc: AllocDelta,
    checksum: u64,
}

static CLOCK_SEQ: AtomicU64 = AtomicU64::new(0);
static CLOCK_SOURCE: AtomicU8 = AtomicU8::new(1);
static LAST_NANOS: AtomicU64 = AtomicU64::new(0);
static LAST_BASE_FRAMES: AtomicU64 = AtomicU64::new(0);
static LAST_CALLBACK_FRAMES: AtomicU64 = AtomicU64::new(0);
static PREV_NANOS: AtomicU64 = AtomicU64::new(0);
static PREV_BASE_FRAMES: AtomicU64 = AtomicU64::new(0);
static PREV_CALLBACK_FRAMES: AtomicU64 = AtomicU64::new(0);
static TOTAL_FRAMES: AtomicU64 = AtomicU64::new(0);

static TIMING_SAMPLE_RATE_HZ: AtomicU32 = AtomicU32::new(0);
static TIMING_DEVICE_PERIOD_NS: AtomicU64 = AtomicU64::new(0);
static TIMING_STREAM_LATENCY_NS: AtomicU64 = AtomicU64::new(0);
static TIMING_BUFFER_FRAMES: AtomicU32 = AtomicU32::new(0);
static TIMING_PADDING_FRAMES: AtomicU32 = AtomicU32::new(0);
static TIMING_QUEUED_FRAMES: AtomicU32 = AtomicU32::new(0);
static TIMING_EST_DELAY_NS: AtomicU64 = AtomicU64::new(0);

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            alloc_calls: AtomicU64::new(0),
            dealloc_calls: AtomicU64::new(0),
            realloc_calls: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
            live_bytes: AtomicU64::new(0),
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

    fn add_live(&self, size: usize) {
        let live = self.live_bytes.fetch_add(size as u64, Ordering::Relaxed) + size as u64;
        update_peak(&self.measure_peak_live_bytes, live);
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

impl PlaybackPosMap {
    fn insert(&mut self, seg: MusicMapSeg) {
        self.backlog_frames = self.backlog_frames.saturating_add(seg.frames);
        self.queue.push_back(seg);
        while self.backlog_frames > 80_000 {
            if let Some(front) = self.queue.pop_front() {
                self.backlog_frames = self.backlog_frames.saturating_sub(front.frames);
            } else {
                self.backlog_frames = 0;
                break;
            }
        }
    }

    fn search(&self, stream_frame: f64) -> Option<(f64, f64)> {
        if self.queue.is_empty() || !stream_frame.is_finite() {
            return None;
        }
        let mut closest = None;
        let mut closest_dist = f64::INFINITY;
        for seg in &self.queue {
            let start = seg.stream_frame_start as f64;
            let end = start + seg.frames as f64;
            if stream_frame >= start && stream_frame < end {
                let diff = stream_frame - start;
                return Some((
                    seg.music_start_sec + diff * seg.music_sec_per_frame,
                    seg.music_sec_per_frame,
                ));
            }
            let start_dist = (stream_frame - start).abs();
            if start_dist < closest_dist {
                closest_dist = start_dist;
                closest = Some((
                    seg.music_start_sec + (stream_frame - start) * seg.music_sec_per_frame,
                    seg.music_sec_per_frame,
                ));
            }
            let end_music = seg.music_start_sec + seg.music_sec_per_frame * seg.frames as f64;
            let end_dist = (stream_frame - end).abs();
            if end_dist < closest_dist {
                closest_dist = end_dist;
                closest = Some((
                    end_music + (stream_frame - end) * seg.music_sec_per_frame,
                    seg.music_sec_per_frame,
                ));
            }
        }
        closest
    }
}

fn main() {
    println!("audio runtime microbench");
    println!("synthetic callback/runtime costs; compare ratios on this machine\n");

    bench_render_mix();
    bench_sfx_queue();
    bench_active_sfx_growth();
    bench_position_map();
    bench_callback_clock();
    bench_backend_timing_proxy();
}

fn bench_render_mix() {
    println!("callback render shape");
    let music = make_i16_samples(CALLBACK_SAMPLES);
    let sfx = make_i16_samples(CALLBACK_SAMPLES + 257);
    let starts = [0usize, 17, 71, 131];
    let mut mix_i16 = vec![0i16; CALLBACK_SAMPLES];
    let mut mix_f32 = vec![0.0f32; CALLBACK_SAMPLES];
    let mut out_i16 = vec![0i16; CALLBACK_SAMPLES];
    let mut out_f32 = vec![0.0f32; CALLBACK_SAMPLES];

    let current_i16_music = bench(
        "i16 out: current music-only f32 roundtrip",
        MIX_ITERS,
        || {
            render_i16_current(
                black_box(&music),
                &[],
                &sfx,
                &mut mix_i16,
                &mut mix_f32,
                &mut out_i16,
            )
        },
    );
    let direct_i16_music = bench("i16 out: direct music-only", MIX_ITERS, || {
        render_i16_direct(black_box(&music), &[], &sfx, &mut out_i16)
    });
    print_result(&current_i16_music);
    print_result(&direct_i16_music);
    print_ratio(
        "direct music-only vs current",
        &current_i16_music,
        &direct_i16_music,
    );

    let current_f32_music = bench("f32 out: current music-only temp+copy", MIX_ITERS, || {
        render_f32_current(
            black_box(&music),
            &[],
            &sfx,
            &mut mix_i16,
            &mut mix_f32,
            &mut out_f32,
        )
    });
    let direct_f32_music = bench("f32 out: direct music-only", MIX_ITERS, || {
        render_f32_direct(black_box(&music), &[], &sfx, &mut out_f32)
    });
    print_result(&current_f32_music);
    print_result(&direct_f32_music);
    print_ratio(
        "direct f32 music-only vs current",
        &current_f32_music,
        &direct_f32_music,
    );

    let current_i16_sfx = bench(
        "i16 out: current 4-sfx f32 roundtrip",
        MIX_SFX_ITERS,
        || {
            render_i16_current(
                black_box(&music),
                black_box(&starts),
                &sfx,
                &mut mix_i16,
                &mut mix_f32,
                &mut out_i16,
            )
        },
    );
    let direct_i16_sfx = bench("i16 out: one-pass 4-sfx final clamp", MIX_SFX_ITERS, || {
        render_i16_direct(black_box(&music), black_box(&starts), &sfx, &mut out_i16)
    });
    print_result(&current_i16_sfx);
    print_result(&direct_i16_sfx);
    print_ratio(
        "one-pass 4-sfx vs current",
        &current_i16_sfx,
        &direct_i16_sfx,
    );

    let current_f32_sfx = bench("f32 out: current 4-sfx temp+copy", MIX_SFX_ITERS, || {
        render_f32_current(
            black_box(&music),
            black_box(&starts),
            &sfx,
            &mut mix_i16,
            &mut mix_f32,
            &mut out_f32,
        )
    });
    let direct_f32_sfx = bench("f32 out: one-pass 4-sfx final clamp", MIX_SFX_ITERS, || {
        render_f32_direct(black_box(&music), black_box(&starts), &sfx, &mut out_f32)
    });
    print_result(&current_f32_sfx);
    print_result(&direct_f32_sfx);
    print_ratio(
        "one-pass f32 4-sfx vs current",
        &current_f32_sfx,
        &direct_f32_sfx,
    );
    println!();
}

fn bench_sfx_queue() {
    println!("SFX queue path");
    let data: Arc<[i16]> = Arc::from(make_i16_samples(512));
    let direct = bench("direct backend channel send+drain", 12, || {
        queue_direct(black_box(QUEUE_MESSAGES), &data)
    });
    let double = bench("current command channel + forward + drain", 12, || {
        queue_double_hop(black_box(QUEUE_MESSAGES), &data)
    });

    print_result(&direct);
    print_result(&double);
    print_ratio("direct channel vs double hop", &double, &direct);
    println!();
}

fn bench_active_sfx_growth() {
    println!("active_sfx vector growth");
    let data: Arc<[i16]> = Arc::from(make_i16_samples(2048));
    let cold = bench("cold Vec::new push 32", ACTIVE_GROWTH_ITERS, || {
        let mut active = Vec::new();
        let mut checksum = 0u64;
        for lane in 0..32 {
            active.push((Arc::clone(&data), lane * 7, lane as u8));
            checksum = checksum.wrapping_add(active.len() as u64);
        }
        checksum
    });
    let presized = bench(
        "new Vec::with_capacity(32) push 32",
        ACTIVE_GROWTH_ITERS,
        || {
            let mut active = Vec::with_capacity(32);
            let mut checksum = 0u64;
            for lane in 0..32 {
                active.push((Arc::clone(&data), lane * 7, lane as u8));
                checksum = checksum.wrapping_add(active.len() as u64);
            }
            checksum
        },
    );
    let mut active = Vec::with_capacity(32);
    let reused = bench(
        "reused Vec capacity 32 push 32",
        ACTIVE_GROWTH_ITERS,
        || {
            active.clear();
            let mut checksum = 0u64;
            for lane in 0..32 {
                active.push((Arc::clone(&data), lane * 7, lane as u8));
                checksum = checksum.wrapping_add(active.len() as u64);
            }
            checksum
        },
    );

    print_result(&cold);
    print_result(&presized);
    print_result(&reused);
    print_ratio("presized vs cold", &cold, &presized);
    print_ratio("reused vs cold", &cold, &reused);
    println!();
}

fn bench_position_map() {
    println!("music position map");
    let map = Mutex::new(make_pos_map(64));
    let double_lock = bench("lookup: current double Mutex lock", POSITION_ITERS, || {
        lookup_double_lock(&map, black_box(31_337.25))
    });
    let single_lock = bench(
        "lookup: drain+search under one lock",
        POSITION_ITERS,
        || lookup_single_lock(&map, black_box(31_337.25)),
    );
    let direct_search = bench(
        "lookup: direct VecDeque search only",
        POSITION_ITERS,
        || {
            let guard = map.lock().unwrap();
            checksum_pos(guard.search(black_box(31_337.25)))
        },
    );

    print_result(&double_lock);
    print_result(&single_lock);
    print_result(&direct_search);
    print_ratio("single lock vs double lock", &double_lock, &single_lock);
    println!();

    let one = make_pos_map(1);
    let many = make_pos_map(512);
    let one_search = bench("search: 1 segment", POSITION_ITERS, || {
        checksum_pos(one.search(black_box(337.25)))
    });
    let many_search = bench("search: 512 segments", POSITION_ITERS / 8, || {
        checksum_pos(many.search(black_box(31_337.25)))
    });
    print_result(&one_search);
    print_result(&many_search);
    println!();
}

fn bench_callback_clock() {
    println!("callback clock atomics");
    let publish = bench("publish callback start+end", CLOCK_ITERS, || {
        publish_callback_clock(black_box(48_000), black_box(1024))
    });
    let snapshot = bench("load callback clock snapshot", CLOCK_ITERS, || {
        load_callback_clock_snapshot()
    });
    let timing_publish = bench("publish output timing atomics", CLOCK_ITERS, || {
        publish_output_timing(48_000, 21_333_333, 21_333_333, 1024, 256, 256, 5_333_333)
    });

    print_result(&publish);
    print_result(&snapshot);
    print_result(&timing_publish);
    println!();
}

fn bench_backend_timing_proxy() {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        println!("backend timing syscall proxy");
        let one_clock = bench("clock_gettime x1", SYSCALL_ITERS, || {
            clock_gettime_nanos().unwrap_or(0)
        });
        let two_clocks = bench("clock_gettime x2", SYSCALL_ITERS, || {
            clock_gettime_nanos()
                .unwrap_or(0)
                .wrapping_add(clock_gettime_nanos().unwrap_or(0))
        });
        print_result(&one_clock);
        print_result(&two_clocks);
        print_ratio("one timestamp vs two timestamps", &two_clocks, &one_clock);
        println!();
    }
}

fn render_i16_current(
    music: &[i16],
    starts: &[usize],
    sfx: &[i16],
    mix_i16: &mut [i16],
    mix_f32: &mut [f32],
    out: &mut [i16],
) -> u64 {
    mix_i16.copy_from_slice(&music[..mix_i16.len()]);
    for (dst, &src) in mix_f32.iter_mut().zip(mix_i16.iter()) {
        *dst = i16_to_f32(src) * MUSIC_VOL;
    }
    for &start in starts {
        for (dst, &src) in mix_f32.iter_mut().zip(&sfx[start..]) {
            *dst = (*dst + i16_to_f32(src) * SFX_VOL).clamp(-1.0, 1.0);
        }
    }
    for (dst, &src) in out.iter_mut().zip(mix_f32.iter()) {
        *dst = f32_to_i16(src);
    }
    checksum_i16(out)
}

fn render_i16_direct(music: &[i16], starts: &[usize], sfx: &[i16], out: &mut [i16]) -> u64 {
    if starts.is_empty() {
        for (dst, &src) in out.iter_mut().zip(music.iter()) {
            *dst = f32_to_i16(i16_to_f32(src) * MUSIC_VOL);
        }
        return checksum_i16(out);
    }
    for i in 0..out.len() {
        let mut acc = i16_to_f32(music[i]) * MUSIC_VOL;
        for &start in starts {
            acc += i16_to_f32(sfx[start + i]) * SFX_VOL;
        }
        out[i] = f32_to_i16(acc.clamp(-1.0, 1.0));
    }
    checksum_i16(out)
}

fn render_f32_current(
    music: &[i16],
    starts: &[usize],
    sfx: &[i16],
    mix_i16: &mut [i16],
    mix_f32: &mut [f32],
    out: &mut [f32],
) -> u64 {
    mix_i16.copy_from_slice(&music[..mix_i16.len()]);
    for (dst, &src) in mix_f32.iter_mut().zip(mix_i16.iter()) {
        *dst = i16_to_f32(src) * MUSIC_VOL;
    }
    for &start in starts {
        for (dst, &src) in mix_f32.iter_mut().zip(&sfx[start..]) {
            *dst = (*dst + i16_to_f32(src) * SFX_VOL).clamp(-1.0, 1.0);
        }
    }
    out.copy_from_slice(&mix_f32[..out.len()]);
    checksum_f32(out)
}

fn render_f32_direct(music: &[i16], starts: &[usize], sfx: &[i16], out: &mut [f32]) -> u64 {
    if starts.is_empty() {
        for (dst, &src) in out.iter_mut().zip(music.iter()) {
            *dst = i16_to_f32(src) * MUSIC_VOL;
        }
        return checksum_f32(out);
    }
    for i in 0..out.len() {
        let mut acc = i16_to_f32(music[i]) * MUSIC_VOL;
        for &start in starts {
            acc += i16_to_f32(sfx[start + i]) * SFX_VOL;
        }
        out[i] = acc.clamp(-1.0, 1.0);
    }
    checksum_f32(out)
}

fn queue_direct(messages: usize, data: &Arc<[i16]>) -> u64 {
    let (tx, rx) = mpsc::channel::<QueuedSfx>();
    for idx in 0..messages {
        tx.send(QueuedSfx {
            data: Arc::clone(data),
            lane: (idx & 1) as u8,
        })
        .unwrap();
    }
    drop(tx);
    let mut checksum = 0u64;
    while let Ok(msg) = rx.recv() {
        checksum = checksum
            .wrapping_add(msg.data.len() as u64)
            .wrapping_add(msg.lane as u64);
    }
    checksum
}

fn queue_double_hop(messages: usize, data: &Arc<[i16]>) -> u64 {
    let (cmd_tx, cmd_rx) = mpsc::channel::<QueuedSfx>();
    let (sfx_tx, sfx_rx) = mpsc::channel::<QueuedSfx>();
    for idx in 0..messages {
        cmd_tx
            .send(QueuedSfx {
                data: Arc::clone(data),
                lane: (idx & 1) as u8,
            })
            .unwrap();
    }
    drop(cmd_tx);
    while let Ok(msg) = cmd_rx.recv() {
        sfx_tx.send(msg).unwrap();
    }
    drop(sfx_tx);
    let mut checksum = 0u64;
    while let Ok(msg) = sfx_rx.recv() {
        checksum = checksum
            .wrapping_add(msg.data.len() as u64)
            .wrapping_add(msg.lane as u64);
    }
    checksum
}

fn lookup_double_lock(map: &Mutex<PlaybackPosMap>, frame: f64) -> u64 {
    {
        let _guard = map.lock().unwrap();
    }
    let guard = map.lock().unwrap();
    checksum_pos(guard.search(frame))
}

fn lookup_single_lock(map: &Mutex<PlaybackPosMap>, frame: f64) -> u64 {
    let guard = map.lock().unwrap();
    checksum_pos(guard.search(frame))
}

fn publish_callback_clock(sample_rate_hz: u64, frames: u64) -> u64 {
    let total_before = TOTAL_FRAMES.load(Ordering::Relaxed);
    let anchor_nanos = total_before
        .saturating_mul(1_000_000_000)
        .saturating_div(sample_rate_hz.max(1));
    CLOCK_SEQ.fetch_add(1, Ordering::AcqRel);
    CLOCK_SOURCE.store(1, Ordering::Relaxed);
    PREV_BASE_FRAMES.store(LAST_BASE_FRAMES.load(Ordering::Relaxed), Ordering::Relaxed);
    PREV_CALLBACK_FRAMES.store(
        LAST_CALLBACK_FRAMES.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    PREV_NANOS.store(LAST_NANOS.load(Ordering::Relaxed), Ordering::Relaxed);
    LAST_BASE_FRAMES.store(total_before, Ordering::Relaxed);
    LAST_CALLBACK_FRAMES.store(0, Ordering::Relaxed);
    LAST_NANOS.store(anchor_nanos.saturating_add(1), Ordering::Relaxed);
    CLOCK_SEQ.fetch_add(1, Ordering::Release);
    CLOCK_SEQ.fetch_add(1, Ordering::AcqRel);
    LAST_CALLBACK_FRAMES.store(frames, Ordering::Relaxed);
    TOTAL_FRAMES.store(total_before.saturating_add(frames), Ordering::Relaxed);
    CLOCK_SEQ.fetch_add(1, Ordering::Release);
    total_before
}

fn load_callback_clock_snapshot() -> u64 {
    loop {
        let seq_start = CLOCK_SEQ.load(Ordering::Acquire);
        if seq_start & 1 != 0 {
            std::hint::spin_loop();
            continue;
        }
        let source = CLOCK_SOURCE.load(Ordering::Relaxed);
        let total = TOTAL_FRAMES.load(Ordering::Relaxed);
        let last_nanos = LAST_NANOS.load(Ordering::Relaxed);
        let last_base = LAST_BASE_FRAMES.load(Ordering::Relaxed);
        let last_frames = LAST_CALLBACK_FRAMES.load(Ordering::Relaxed);
        let prev_nanos = PREV_NANOS.load(Ordering::Relaxed);
        let prev_base = PREV_BASE_FRAMES.load(Ordering::Relaxed);
        let prev_frames = PREV_CALLBACK_FRAMES.load(Ordering::Relaxed);
        let seq_end = CLOCK_SEQ.load(Ordering::Acquire);
        if seq_start == seq_end {
            return source as u64
                ^ total
                ^ last_nanos
                ^ last_base
                ^ last_frames
                ^ prev_nanos
                ^ prev_base
                ^ prev_frames;
        }
    }
}

fn publish_output_timing(
    sample_rate_hz: u32,
    device_period_ns: u64,
    stream_latency_ns: u64,
    buffer_frames: u32,
    padding_frames: u32,
    queued_frames: u32,
    estimated_output_delay_ns: u64,
) -> u64 {
    TIMING_SAMPLE_RATE_HZ.store(sample_rate_hz, Ordering::Relaxed);
    TIMING_DEVICE_PERIOD_NS.store(device_period_ns, Ordering::Relaxed);
    TIMING_STREAM_LATENCY_NS.store(stream_latency_ns, Ordering::Relaxed);
    TIMING_BUFFER_FRAMES.store(buffer_frames, Ordering::Relaxed);
    TIMING_PADDING_FRAMES.store(padding_frames, Ordering::Relaxed);
    TIMING_QUEUED_FRAMES.store(queued_frames, Ordering::Relaxed);
    TIMING_EST_DELAY_NS.store(estimated_output_delay_ns, Ordering::Relaxed);
    u64::from(sample_rate_hz)
        ^ device_period_ns
        ^ stream_latency_ns
        ^ u64::from(buffer_frames)
        ^ u64::from(padding_frames)
        ^ u64::from(queued_frames)
        ^ estimated_output_delay_ns
}

#[cfg(all(unix, not(target_os = "macos")))]
fn clock_gettime_nanos() -> Option<u64> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `clock_gettime` writes into the stack-local `timespec`.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    if rc != 0 || ts.tv_sec < 0 || ts.tv_nsec < 0 {
        return None;
    }
    Some((ts.tv_sec as u64).saturating_mul(1_000_000_000) + ts.tv_nsec as u64)
}

fn make_pos_map(segments: usize) -> PlaybackPosMap {
    let mut map = PlaybackPosMap::default();
    for idx in 0..segments {
        let frames = 1024;
        map.insert(MusicMapSeg {
            stream_frame_start: (idx * frames) as i64,
            frames: frames as i64,
            music_start_sec: idx as f64 * 1024.0 / 48_000.0,
            music_sec_per_frame: 1.0 / 48_000.0,
        });
    }
    map
}

fn make_i16_samples(len: usize) -> Vec<i16> {
    let mut out = Vec::with_capacity(len);
    let mut x = 0x9e37_79b9u32;
    for i in 0..len {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        let sample = ((x as i32).wrapping_add((i as i32) * 97) >> 16) as i16;
        out.push(sample);
    }
    out
}

#[inline(always)]
fn i16_to_f32(sample: i16) -> f32 {
    sample as f32 / 32768.0
}

#[inline(always)]
fn f32_to_i16(sample: f32) -> i16 {
    let sample = sample.clamp(-1.0, 1.0);
    if sample >= 1.0 {
        i16::MAX
    } else if sample <= -1.0 {
        i16::MIN
    } else {
        (sample * 32768.0) as i16
    }
}

fn checksum_i16(samples: &[i16]) -> u64 {
    let mut out = 0u64;
    for &sample in samples {
        out = out
            .wrapping_mul(131)
            .wrapping_add((sample as i32 as u32) as u64);
    }
    out
}

fn checksum_f32(samples: &[f32]) -> u64 {
    let mut out = 0u64;
    for &sample in samples {
        out = out.wrapping_mul(131).wrapping_add(sample.to_bits() as u64);
    }
    out
}

fn checksum_pos(pos: Option<(f64, f64)>) -> u64 {
    match pos {
        Some((a, b)) => a.to_bits() ^ b.to_bits().rotate_left(17),
        None => 0,
    }
}

fn bench<F>(name: impl Into<String>, iters: usize, mut f: F) -> BenchResult
where
    F: FnMut() -> u64,
{
    let name = name.into();
    let mut checksum = 0u64;
    for _ in 0..32 {
        checksum = checksum.wrapping_add(black_box(f()));
    }
    let start_alloc = ALLOC.begin_measurement();
    let started = Instant::now();
    for _ in 0..iters {
        checksum = checksum.wrapping_add(black_box(f()));
    }
    BenchResult {
        name,
        iters,
        elapsed: started.elapsed(),
        alloc: ALLOC.snapshot().diff(start_alloc),
        checksum,
    }
}

fn print_result(result: &BenchResult) {
    let total_ms = result.elapsed.as_secs_f64() * 1000.0;
    let per_iter_us = result.elapsed.as_secs_f64() * 1_000_000.0 / result.iters as f64;
    println!(
        concat!(
            "{:<46} {:>9.3} ms total {:>9.3} us/iter ",
            "alloc={} dealloc={} realloc={} bytes={} freed={} live={} peak={} checksum={}"
        ),
        result.name,
        total_ms,
        per_iter_us,
        result.alloc.alloc_calls,
        result.alloc.dealloc_calls,
        result.alloc.realloc_calls,
        result.alloc.alloc_bytes,
        result.alloc.free_bytes,
        result.alloc.live_bytes,
        result.alloc.peak_live_delta,
        result.checksum,
    );
}

fn print_ratio(label: &str, base: &BenchResult, candidate: &BenchResult) {
    let base_s = base.elapsed.as_secs_f64();
    let candidate_s = candidate.elapsed.as_secs_f64();
    if candidate_s == 0.0 {
        println!("  {label}: candidate time rounded to zero");
        return;
    }
    println!(
        "  {label}: {:.2}x ({:.1}% of base), alloc calls {} -> {}",
        base_s / candidate_s,
        candidate_s * 100.0 / base_s.max(f64::MIN_POSITIVE),
        base.alloc.alloc_calls,
        candidate.alloc.alloc_calls,
    );
}

fn update_peak(target: &AtomicU64, value: u64) {
    let mut current = target.load(Ordering::Relaxed);
    while value > current {
        match target.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

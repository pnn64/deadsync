use deadsync::core::input::{PadCode, PadEvent, PadId, RawKeyboardEvent};
use std::alloc::{GlobalAlloc, Layout, System};
use std::error::Error;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
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
    max_inflight: usize,
}

#[derive(Clone, Copy)]
enum Scenario {
    Key,
    Pad,
    Both,
}

#[derive(Clone, Copy)]
enum Mode {
    Direct,
    Proxy,
}

#[derive(Clone, Copy, Debug)]
enum BenchEvent {
    Start,
    Key(RawKeyboardEvent),
    Pad(PadEvent),
}

#[derive(Clone, Copy)]
struct BenchmarkResult {
    scenario: &'static str,
    mode: Mode,
    iters: u64,
    warmup: u64,
    max_inflight: Option<usize>,
    elapsed_s: f64,
    producer_send_s: Option<f64>,
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
            "both" => Ok(Self::Both),
            _ => Err(
                format!("unknown --scenario value '{value}', expected key, pad, or both").into(),
            ),
        }
    }
}

impl Mode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Proxy => "proxy",
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    match args.scenario {
        Scenario::Key => run_case("key", sample_key_event(), &args)?,
        Scenario::Pad => run_case("pad", sample_pad_event(), &args)?,
        Scenario::Both => {
            run_case("key", sample_key_event(), &args)?;
            run_case("pad", sample_pad_event(), &args)?;
        }
    }
    Ok(())
}

fn run_case(name: &'static str, ev: BenchEvent, args: &Args) -> Result<(), Box<dyn Error>> {
    print_result(bench_direct(name, ev, args));
    print_result(bench_proxy(name, ev, args)?);
    Ok(())
}

fn bench_direct(name: &'static str, ev: BenchEvent, args: &Args) -> BenchmarkResult {
    let mut checksum = 0u64;
    for _ in 0..args.warmup {
        checksum = mix_checksum(checksum, consume_event(ev));
    }
    let start_alloc = ALLOC.begin_measurement();
    let started = Instant::now();
    for _ in 0..args.iters {
        checksum = mix_checksum(checksum, consume_event(black_box(ev)));
    }
    let elapsed = started.elapsed();
    let end_alloc = ALLOC.snapshot();
    BenchmarkResult {
        scenario: name,
        mode: Mode::Direct,
        iters: args.iters,
        warmup: args.warmup,
        max_inflight: None,
        elapsed_s: elapsed.as_secs_f64(),
        producer_send_s: None,
        alloc: end_alloc.diff(start_alloc),
        checksum,
    }
}

fn bench_proxy(
    name: &'static str,
    ev: BenchEvent,
    args: &Args,
) -> Result<BenchmarkResult, Box<dyn Error>> {
    let event_loop: EventLoop<BenchEvent> = EventLoop::<BenchEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let (start_tx, start_rx) = mpsc::channel::<()>();
    let max_inflight = args.max_inflight.max(1);
    let (ack_tx, ack_rx) = mpsc::sync_channel::<()>(max_inflight);
    let send_elapsed = Arc::new(Mutex::new(None::<f64>));
    let send_elapsed_out = Arc::clone(&send_elapsed);
    let iters = args.iters;
    let warmup = args.warmup;
    let producer = thread::spawn(move || {
        run_proxy_producer(
            proxy,
            ev,
            warmup,
            iters,
            max_inflight,
            start_rx,
            ack_rx,
            send_elapsed_out,
        )
    });

    let mut app = ProxyBenchApp::new(args.iters, start_tx, ack_tx);
    event_loop.run_app(&mut app)?;
    let producer_result = match producer.join() {
        Ok(Ok(elapsed)) => elapsed,
        Ok(Err(err)) => return Err(err.to_string().into()),
        Err(_) => return Err("proxy producer thread panicked".into()),
    };
    let producer_send_s = send_elapsed.lock().unwrap().or(Some(producer_result));
    let result = BenchmarkResult {
        scenario: name,
        mode: Mode::Proxy,
        iters: args.iters,
        warmup: args.warmup,
        max_inflight: Some(max_inflight),
        elapsed_s: app
            .elapsed_s
            .ok_or("proxy benchmark did not record an elapsed time")?,
        producer_send_s,
        alloc: app
            .alloc_delta
            .ok_or("proxy benchmark did not capture allocation stats")?,
        checksum: app.checksum,
    };
    Ok(result)
}

fn run_proxy_producer(
    proxy: EventLoopProxy<BenchEvent>,
    ev: BenchEvent,
    warmup: u64,
    iters: u64,
    max_inflight: usize,
    start_rx: mpsc::Receiver<()>,
    ack_rx: mpsc::Receiver<()>,
    send_elapsed: Arc<Mutex<Option<f64>>>,
) -> Result<f64, Box<dyn Error + Send + Sync>> {
    start_rx.recv()?;
    send_windowed(&proxy, ev, warmup, max_inflight, &ack_rx)?;
    let started = Instant::now();
    proxy.send_event(BenchEvent::Start)?;
    send_windowed(&proxy, ev, iters, max_inflight, &ack_rx)?;
    let elapsed = started.elapsed().as_secs_f64();
    *send_elapsed.lock().unwrap() = Some(elapsed);
    Ok(elapsed)
}

fn send_windowed(
    proxy: &EventLoopProxy<BenchEvent>,
    ev: BenchEvent,
    total: u64,
    max_inflight: usize,
    ack_rx: &mpsc::Receiver<()>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let window = max_inflight.max(1) as u64;
    let mut sent = 0u64;
    let mut acked = 0u64;
    while sent < total {
        while sent < total && sent.saturating_sub(acked) < window {
            proxy.send_event(ev)?;
            sent += 1;
        }
        ack_rx.recv()?;
        acked += 1;
    }
    while acked < sent {
        ack_rx.recv()?;
        acked += 1;
    }
    Ok(())
}

struct ProxyBenchApp {
    target: u64,
    started: Option<Instant>,
    elapsed_s: Option<f64>,
    alloc_start: Option<AllocSnapshot>,
    alloc_delta: Option<AllocDelta>,
    checksum: u64,
    start_tx: Option<mpsc::Sender<()>>,
    ack_tx: mpsc::SyncSender<()>,
    seen_start: bool,
    received: u64,
}

impl ProxyBenchApp {
    fn new(target: u64, start_tx: mpsc::Sender<()>, ack_tx: mpsc::SyncSender<()>) -> Self {
        Self {
            target,
            started: None,
            elapsed_s: None,
            alloc_start: None,
            alloc_delta: None,
            checksum: 0,
            start_tx: Some(start_tx),
            ack_tx,
            seen_start: false,
            received: 0,
        }
    }
}

impl ApplicationHandler<BenchEvent> for ProxyBenchApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
        if let Some(start_tx) = self.start_tx.take() {
            let _ = start_tx.send(());
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: BenchEvent) {
        match event {
            BenchEvent::Start => {
                self.seen_start = true;
                self.received = 0;
                self.checksum = 0;
                self.alloc_start = Some(ALLOC.begin_measurement());
                self.started = Some(Instant::now());
            }
            _ if !self.seen_start => {
                self.checksum = mix_checksum(self.checksum, consume_event(event));
                let _ = self.ack_tx.send(());
            }
            _ => {
                self.checksum = mix_checksum(self.checksum, consume_event(event));
                self.received += 1;
                let _ = self.ack_tx.send(());
                if self.received >= self.target {
                    if let Some(started) = self.started.take() {
                        self.elapsed_s = Some(started.elapsed().as_secs_f64());
                    }
                    if let Some(start) = self.alloc_start.take() {
                        self.alloc_delta = Some(ALLOC.snapshot().diff(start));
                    }
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        _event: winit::event::WindowEvent,
    ) {
    }
}

#[inline(always)]
fn consume_event(ev: BenchEvent) -> u64 {
    match ev {
        BenchEvent::Start => 0,
        BenchEvent::Key(ev) => {
            black_box(ev);
            u64::from(ev.pressed) ^ (ev.host_nanos.wrapping_mul(3)) ^ key_hash(ev.code)
        }
        BenchEvent::Pad(ev) => {
            black_box(ev);
            pad_hash(ev)
        }
    }
}

#[inline(always)]
fn key_hash(code: KeyCode) -> u64 {
    match code {
        KeyCode::ArrowLeft => 1,
        KeyCode::ArrowDown => 2,
        KeyCode::ArrowUp => 3,
        KeyCode::ArrowRight => 4,
        KeyCode::Enter => 5,
        _ => 9,
    }
}

#[inline(always)]
fn pad_hash(ev: PadEvent) -> u64 {
    match ev {
        PadEvent::Dir {
            id, dir, pressed, ..
        } => (usize::from(id) as u64) << 8 | (dir.ix() as u64) << 1 | u64::from(pressed),
        PadEvent::RawButton {
            id,
            code,
            pressed,
            value,
            ..
        } => {
            ((usize::from(id) as u64) << 16)
                ^ u64::from(code.into_u32())
                ^ u64::from(pressed)
                ^ value.to_bits() as u64
        }
        PadEvent::RawAxis {
            id, code, value, ..
        } => ((usize::from(id) as u64) << 16) ^ u64::from(code.into_u32()) ^ value.to_bits() as u64,
    }
}

fn sample_key_event() -> BenchEvent {
    BenchEvent::Key(RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed: true,
        repeat: false,
        timestamp: Instant::now(),
        host_nanos: 123_456_789,
    })
}

fn sample_pad_event() -> BenchEvent {
    BenchEvent::Pad(PadEvent::RawButton {
        id: PadId(1),
        timestamp: Instant::now(),
        host_nanos: 123_456_789,
        code: PadCode(7),
        uuid: [1; 16],
        value: 1.0,
        pressed: true,
    })
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
    let mut scenario = Scenario::Both;
    let mut iters = 200_000u64;
    let mut warmup = 10_000u64;
    let mut max_inflight = 64usize;
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
            "--max-inflight" => {
                max_inflight = args
                    .next()
                    .ok_or("--max-inflight requires a value")?
                    .parse::<usize>()?;
            }
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
        max_inflight,
    })
}

fn print_help() {
    println!(
        "usage: cargo run --bin input_bench -- [--scenario key|pad|both] [--iters N] [--warmup N] [--max-inflight N]"
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
    println!("mode: {}", result.mode.as_str());
    if let Some(max_inflight) = result.max_inflight {
        println!("window: max_inflight={max_inflight}");
    }
    println!(
        "time: warmup={} iters={} total={:.6}s per_iter={:.3}us events/s={:.0}",
        result.warmup, result.iters, result.elapsed_s, per_iter_us, events_per_s
    );
    if let Some(send_s) = result.producer_send_s {
        let send_per_iter_us = if result.iters == 0 {
            0.0
        } else {
            send_s * 1_000_000.0 / result.iters as f64
        };
        println!(
            "producer: total={:.6}s per_iter={:.3}us",
            send_s, send_per_iter_us
        );
    }
    println!(
        "alloc: allocs/iter={:.3} bytes/iter={:.1} live_delta={} peak_live_delta={}",
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

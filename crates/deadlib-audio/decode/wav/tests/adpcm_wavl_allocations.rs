use deadlib_audio_decode_wav::{AdpcmData, AdpcmWavl, Muxer, Wave, WavlMuxer, WriteOptions};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    io::Cursor,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
};
static TRACK: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
struct Heap;
fn allocated(n: usize) {
    if TRACK.load(Relaxed) {
        CALLS.fetch_add(1, Relaxed);
        BYTES.fetch_add(n, Relaxed);
    }
}
// SAFETY: allocation pointers and layouts are forwarded unchanged to System.
// The counters do not allocate and cannot unwind.
unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            allocated(l.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() {
            allocated(l.size());
        }
        p
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let r = unsafe { System.realloc(p, l, n) };
        if !r.is_null() {
            allocated(n);
        }
        r
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static ALLOCATOR: Heap = Heap;

fn measure<T>(f: impl FnOnce() -> T) -> (T, usize, usize) {
    CALLS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    TRACK.store(true, Relaxed);
    let value = f();
    TRACK.store(false, Relaxed);
    (value, CALLS.load(Relaxed), BYTES.load(Relaxed))
}

#[test]
fn borrowed_sequence_index_is_compact_reads_allocate_nothing_and_flat_state_is_unchanged() {
    let format = [
        17, 0, 2, 0, 0x40, 0x1f, 0, 0, 0x8e, 0x37, 0, 0, 16, 0, 4, 0, 2, 0, 9, 0,
    ];
    let mut mux = WavlMuxer::new(
        Cursor::new(Vec::new()),
        &format,
        WriteOptions::default(),
        &[],
    )
    .unwrap();
    for _ in 0..4 {
        mux.write_data(&[0; 16], 9).unwrap();
        mux.write_silence(3).unwrap();
    }
    let bytes = mux.finish().unwrap().into_inner();
    let wave = Wave::parse(&bytes).unwrap();
    let (audio, calls, bytes) = measure(|| AdpcmWavl::new(black_box(&wave)).unwrap());
    assert_eq!((calls, bytes), (1, 128)); // 16 bytes for each of eight inventoried children.
    let ((), calls, bytes) = measure(|| {
        for start in [0, 8, 9, 10, 11, 12, 47, 48] {
            let mut out = [1234; 25];
            let n = audio
                .read_frames(black_box(start), &mut out, &mut [0; 18])
                .unwrap();
            assert!(out[..n].iter().all(|&n| n == 0));
            assert!(out[n..].iter().all(|&n| n == 1234));
        }
    });
    assert_eq!((calls, bytes), (0, 0));
    let mut mux = Muxer::new(
        Cursor::new(Vec::new()),
        &format,
        WriteOptions::default(),
        &[],
    )
    .unwrap();
    mux.write_data(&[0; 16], 9).unwrap();
    let flat = mux.finish().unwrap().into_inner();
    let wave = Wave::parse(&flat).unwrap();
    let (_, calls, bytes) = measure(|| AdpcmData::new(black_box(&wave)).unwrap());
    assert_eq!((calls, bytes), (0, 0));
    println!(
        "wavl index: 1 requested allocation/128 bytes for 8 children; reads and flat views: 0; object bytes wavl={} flat={}",
        std::mem::size_of::<AdpcmWavl<'_>>(),
        std::mem::size_of::<AdpcmData<'_>>()
    );
}

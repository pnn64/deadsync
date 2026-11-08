use deadlib_audio_decode_wav::{Error, ForwardReader, Pcm, Reader, Writer};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    fmt::Debug,
    io::{Read, Seek, Write},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    time::Instant,
};
pub struct Heap;
static TRACK: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static REQUESTED: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn allocated(n: usize) {
    CALLS.fetch_add(1, Relaxed);
    REQUESTED.fetch_add(n, Relaxed);
    let live = LIVE.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(live, Relaxed);
}
// SAFETY: all pointer/layout contracts are forwarded unchanged to System.
// The atomic observer never allocates, and tracking brackets complete operations.
unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() && TRACK.load(Relaxed) {
            allocated(l.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() && TRACK.load(Relaxed) {
            allocated(l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if TRACK.load(Relaxed) {
            LIVE.fetch_sub(l.size(), Relaxed);
        }
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, n) };
        if !q.is_null() && TRACK.load(Relaxed) {
            LIVE.fetch_sub(l.size(), Relaxed);
            allocated(n);
        }
        q
    }
}
#[cfg(windows)]
pub fn cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut core::ffi::c_void;
        fn QueryThreadCycleTime(t: *mut core::ffi::c_void, n: *mut u64) -> i32;
    }
    let mut n = 0;
    // SAFETY: the pseudo-handle is valid and n is writable during this call.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut n) },
        0
    );
    n
}
#[cfg(not(windows))]
pub fn cycles() -> u64 {
    0
}
pub fn bench(name: &str, iters: usize, wire: usize, output: usize, mut work: impl FnMut()) {
    work();
    CALLS.store(0, Relaxed);
    REQUESTED.store(0, Relaxed);
    LIVE.store(0, Relaxed);
    PEAK.store(0, Relaxed);
    TRACK.store(true, Relaxed);
    work();
    TRACK.store(false, Relaxed);
    let calls = CALLS.load(Relaxed);
    let requested = REQUESTED.load(Relaxed);
    let peak = PEAK.load(Relaxed);
    assert_eq!(LIVE.load(Relaxed), 0);
    for trial in 0..3 {
        let start = Instant::now();
        let c = cycles();
        for _ in 0..iters {
            work();
        }
        let cpu = cycles() - c;
        let ns = start.elapsed().as_nanos() as f64 / iters as f64;
        println!(
            "{name}\t{trial}\t{iters}\t{ns:.3}\t{:.3}\t{:.3}\t{:.3}\t{calls}\t{requested}\t{peak}",
            cpu as f64 / iters as f64,
            wire as f64 / (1 << 20) as f64 * 1e9 / ns,
            output as f64 / (1 << 20) as f64 * 1e9 / ns
        );
    }
}

pub trait Numeric: Copy + Default + Debug {
    const WIDTH: usize;
    fn bits(self) -> u64;
    fn word(b: &[u8]) -> Self;
    fn pcm(p: Pcm<'_>, start: u64, out: &mut [Self]) -> Result<usize, Error>;
    fn reader<R: Read + Seek>(r: &mut Reader<R>, out: &mut [Self]) -> Result<usize, Error>;
    fn forward<R: Read>(r: &mut ForwardReader<R>, out: &mut [Self]) -> Result<usize, Error>;
    fn write<W: Write + Seek>(w: &mut Writer<W>, out: &[Self]) -> Result<(), Error>;
    fn hound<R: Read>(r: &mut hound::WavReader<R>, out: &mut [Self])
    -> Result<usize, hound::Error>;
    fn hound_write<W: Write + Seek>(
        w: &mut hound::WavWriter<W>,
        out: &[Self],
    ) -> Result<(), hound::Error>;
}
impl Numeric for i32 {
    const WIDTH: usize = 4;
    fn bits(self) -> u64 {
        self as u32 as u64
    }
    fn word(b: &[u8]) -> Self {
        i32::from_le_bytes(b.try_into().unwrap())
    }
    fn pcm(p: Pcm<'_>, start: u64, out: &mut [Self]) -> Result<usize, Error> {
        p.read_i32(start, out)
    }
    fn reader<R: Read + Seek>(r: &mut Reader<R>, out: &mut [Self]) -> Result<usize, Error> {
        r.read_integers(out)
    }
    fn forward<R: Read>(r: &mut ForwardReader<R>, out: &mut [Self]) -> Result<usize, Error> {
        r.read_integers(out)
    }
    fn write<W: Write + Seek>(w: &mut Writer<W>, out: &[Self]) -> Result<(), Error> {
        w.write_integers(out)
    }
    fn hound<R: Read>(
        r: &mut hound::WavReader<R>,
        out: &mut [Self],
    ) -> Result<usize, hound::Error> {
        let mut n = 0;
        for (o, v) in out.iter_mut().zip(r.samples::<Self>()) {
            *o = v?;
            n += 1;
        }
        Ok(n)
    }
    fn hound_write<W: Write + Seek>(
        w: &mut hound::WavWriter<W>,
        out: &[Self],
    ) -> Result<(), hound::Error> {
        for &v in out {
            w.write_sample(v)?;
        }
        Ok(())
    }
}
macro_rules! float {
    ($t:ty,$read:ident,$write:ident,$word:ty,$width:literal) => {
        impl Numeric for $t {
            const WIDTH: usize = $width;
            fn bits(self) -> u64 {
                self.to_bits() as u64
            }
            fn word(b: &[u8]) -> Self {
                Self::from_bits(<$word>::from_le_bytes(b.try_into().unwrap()))
            }
            fn pcm(p: Pcm<'_>, start: u64, out: &mut [Self]) -> Result<usize, Error> {
                p.$read(start, out)
            }
            fn reader<R: Read + Seek>(r: &mut Reader<R>, out: &mut [Self]) -> Result<usize, Error> {
                r.$read(out)
            }
            fn forward<R: Read>(
                r: &mut ForwardReader<R>,
                out: &mut [Self],
            ) -> Result<usize, Error> {
                r.$read(out)
            }
            fn write<W: Write + Seek>(w: &mut Writer<W>, out: &[Self]) -> Result<(), Error> {
                w.$write(out)
            }
            fn hound<R: Read>(
                r: &mut hound::WavReader<R>,
                out: &mut [Self],
            ) -> Result<usize, hound::Error> {
                hound_float(r, out)
            }
            fn hound_write<W: Write + Seek>(
                w: &mut hound::WavWriter<W>,
                out: &[Self],
            ) -> Result<(), hound::Error> {
                hound_float_write(w, out)
            }
        }
    };
}
trait HoundFloat: Copy {
    fn fill<R: Read>(r: &mut hound::WavReader<R>, out: &mut [Self]) -> Result<usize, hound::Error>;
    fn write<W: Write + Seek>(
        w: &mut hound::WavWriter<W>,
        out: &[Self],
    ) -> Result<(), hound::Error>;
}
impl HoundFloat for f32 {
    fn fill<R: Read>(r: &mut hound::WavReader<R>, out: &mut [Self]) -> Result<usize, hound::Error> {
        let mut n = 0;
        for (o, v) in out.iter_mut().zip(r.samples::<Self>()) {
            *o = v?;
            n += 1;
        }
        Ok(n)
    }
    fn write<W: Write + Seek>(
        w: &mut hound::WavWriter<W>,
        out: &[Self],
    ) -> Result<(), hound::Error> {
        for &v in out {
            w.write_sample(v)?;
        }
        Ok(())
    }
}
impl HoundFloat for f64 {
    fn fill<R: Read>(_: &mut hound::WavReader<R>, _: &mut [Self]) -> Result<usize, hound::Error> {
        Err(hound::Error::Unsupported)
    }
    fn write<W: Write + Seek>(_: &mut hound::WavWriter<W>, _: &[Self]) -> Result<(), hound::Error> {
        Err(hound::Error::Unsupported)
    }
}
fn hound_float<T: HoundFloat, R: Read>(
    r: &mut hound::WavReader<R>,
    out: &mut [T],
) -> Result<usize, hound::Error> {
    T::fill(r, out)
}
fn hound_float_write<T: HoundFloat, W: Write + Seek>(
    w: &mut hound::WavWriter<W>,
    out: &[T],
) -> Result<(), hound::Error> {
    T::write(w, out)
}
float!(f32, read_f32, write_f32, u32, 4);
float!(f64, read_f64, write_f64, u64, 8);
pub fn check<T: Numeric>(out: &[T], period: &[T], start: usize) {
    for (i, &v) in out.iter().enumerate() {
        assert_eq!(
            v.bits(),
            period[(start + i) % period.len()].bits(),
            "sample {}",
            start + i
        );
    }
}
pub fn period<T: Numeric>(b: &[u8]) -> Vec<T> {
    assert!(b.len().is_multiple_of(T::WIDTH));
    b.chunks(T::WIDTH).map(T::word).collect()
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Memory {
    pub cb: u32,
    pub faults: u32,
    pub peak_working_set: usize,
    pub working_set: usize,
    pub peak_paged: usize,
    pub paged: usize,
    pub peak_nonpaged: usize,
    pub nonpaged: usize,
    pub pagefile: usize,
    pub peak_pagefile: usize,
    pub private_bytes: usize,
}
#[cfg(windows)]
pub fn memory() -> Memory {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(p: *mut core::ffi::c_void, m: *mut Memory, n: u32) -> i32;
    }
    let mut m = Memory {
        cb: std::mem::size_of::<Memory>() as u32,
        ..Memory::default()
    };
    // SAFETY: the process handle is valid and m has the advertised C layout/size.
    assert_ne!(
        unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut m,
                std::mem::size_of::<Memory>() as u32,
            )
        },
        0
    );
    m
}
#[cfg(not(windows))]
pub fn memory() -> Memory {
    Memory::default()
}

use codec::{nal::{parse_annex_b, parse_avcc, parse_sps_info}, OrderedDecoder, SurfaceDecoder};
#[macro_use]
extern crate alloc;
use std::{alloc::{GlobalAlloc, Layout, System}, ffi::c_void, hint::black_box, sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed}, io::{Write, BufWriter}};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThread() -> *mut c_void;
    fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
    fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
}
fn cycles() -> u64 {
    let mut value = 0;
    assert_ne!(unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut value) }, 0);
    value
}
#[cfg(allocations)]
#[global_allocator]
static ALLOC: Meter = Meter;
#[allow(dead_code)]
struct Meter;
static ENABLED: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
fn count(p: *mut u8, size: usize) {
    if ENABLED.load(Relaxed) && !p.is_null() && size != 0 {
        CALLS.fetch_add(1, Relaxed);
        BYTES.fetch_add(size as u64, Relaxed);
    }
}
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) }; count(p, l.size()); p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) }; count(p, l.size()); p
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, l, n) }; count(p, n); p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) { unsafe { System.dealloc(p, l) } }
}
#[allow(dead_code)]
#[path = "bitstream.rs"]
mod bitstream;
#[allow(dead_code)]
mod cavlc;
#[allow(dead_code)]
mod once;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let cpu: usize = args[1].parse().unwrap();
    assert_ne!(unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << cpu) }, 0);
    let mode = &args[2];
    let input = std::fs::read(&args[3]).unwrap();
    let repeats: usize = args[4].parse().unwrap();
    let mut output = args.get(5).map(|p| BufWriter::new(std::fs::File::create(p).unwrap()));
    let mut units = 0u64;
    let mut signature = 0u64;
    let avcc = mode.ends_with("avcc");
    let parse = |b| if avcc { parse_avcc(b, 4) } else { parse_annex_b(b) };
    let mut records = Vec::new();
    if matches!(mode.as_str(), "residual" | "ue" | "bits") {
        let tag = match mode.as_str() { "residual" => 0, "ue" => 1, _ => 2 };
        let mut tail = input.as_slice();
        while !tail.is_empty() {
            let (header, rest) = tail.split_at(9);
            let len = u16::from_le_bytes([header[7], header[8]]) as usize;
            let (data, rest) = rest.split_at(len);
            tail = rest;
            if header[0] == tag {
                let param = i32::from_le_bytes(header[1..5].try_into().unwrap());
                records.push((param, header[5], header[6], data));
            }
        }
        assert!(!records.is_empty(), "no trace records for {mode}");
    }
    CALLS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    ENABLED.store(true, Relaxed);
    let start = cycles();
    for _ in 0..repeats {
        if !records.is_empty() {
            for &(param, width, offset, data) in black_box(&records) {
                let mut reader = bitstream::BitstreamReader::new(data);
                reader.skip_bits(offset);
                let value = match mode.as_str() {
                    "residual" => {
                        let mut coeffs = [0i32; 16];
                        let count = cavlc::parse_residual_block_cavlc(&mut reader, &mut coeffs, width as usize, param);
                        let hash = coeffs.iter().fold(0u64, |sum, &c| sum.wrapping_mul(31).wrapping_add(c as u64));
                        black_box(coeffs);
                        hash.wrapping_add(count.map_or(u64::MAX, u64::from))
                    }
                    "ue" => reader.read_ue().map_or(u64::MAX, u64::from),
                    _ => reader.read_bits(width).map_or(u64::MAX, u64::from),
                };
                let pos = reader.position();
                signature = signature.wrapping_add(value).wrapping_add((pos.0 * 8 + pos.1 as usize) as u64);
                units += 1;
            }
        } else if mode.starts_with("parse") {
            let nals = parse(black_box(&input));
            for n in &nals {
                units += n.rbsp.len() as u64;
                signature = signature.wrapping_add(n.rbsp.len() as u64 + n.nal_ref_idc as u64);
                black_box(&n.rbsp);
            }
        } else if mode == "peek" {
            let mut reader = bitstream::BitstreamReader::new(black_box(&input));
            while reader.bits_remaining() >= 24 {
                for n in [16, 14, 10, 8, 13, 9] {
                    signature = signature.wrapping_add(reader.peek_bits(n) as u64);
                    reader.skip_bits(3);
                    units += 1;
                }
            }
        } else if mode.starts_with("surface") {
            let nals = parse(black_box(&input));
            let depth = nals.iter().filter_map(|n| parse_sps_info(n).ok()).map(|s| s.max_reorder_frames.unwrap_or(16)).max().unwrap_or(16);
            let mut decoder = SurfaceDecoder::with_max_depth(depth as usize);
            let mut consume = |frame: codec::SurfaceFrame| {
                units += 1;
                let (w,h) = frame.size();
                signature += u64::from(w) * u64::from(h);
                for i in 0..3 {
                    let p = frame.plane(i).unwrap();
                    if let Some(out) = &mut output {
                        for row in p.rows() { out.write_all(row).unwrap(); }
                    } else { black_box(p); }
                }
            };
            for nal in &nals { decoder.decode_nal(nal, &mut consume).unwrap(); }
            decoder.flush(consume);
        } else {
            let mut decoder = OrderedDecoder::new();
            let mut consume = |frame: codec::Frame| {
                units += 1;
                signature += u64::from(frame.width) * u64::from(frame.height);
                for plane in [&frame.y,&frame.u,&frame.v] {
                    if let Some(out) = &mut output { out.write_all(plane).unwrap(); }
                    else { black_box(plane); }
                }
            };
            for nal in parse(black_box(&input)) {
                for frame in decoder.decode_nal(&nal).unwrap() { consume(frame); }
            }
            for frame in decoder.flush() { consume(frame); }
        }
    }
    let elapsed = cycles() - start;
    ENABLED.store(false, Relaxed);
    println!("{units},{signature},{elapsed},{},{}", CALLS.load(Relaxed), BYTES.load(Relaxed));
}

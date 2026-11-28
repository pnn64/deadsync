// Included only in temporary copies of picture.rs by the benchmark runner.
pub struct PerfMotion<'a> {
    predictor: i32,
    code: u8,
    offset: u8,
    bytes: &'a [u8],
}

impl<'a> PerfMotion<'a> {
    pub fn parse(mut data: &'a [u8]) -> Vec<Self> {
        let mut records = Vec::new();
        while !data.is_empty() {
            let (header, rest) = data.split_first_chunk::<7>().unwrap();
            let (bytes, rest) = rest.split_at(header[6] as usize);
            records.push(Self {
                predictor: i32::from_le_bytes(header[..4].try_into().unwrap()),
                code: header[4],
                offset: header[5],
                bytes,
            });
            data = rest;
        }
        records
    }

    fn run(&self) -> (i32, usize) {
        let mut bits = Bits::new(self.bytes);
        bits.skip(self.offset).unwrap();
        let value = motion_component(&mut bits, self.predictor, self.code).unwrap();
        (value, bits.perf_position())
    }
}

pub fn perf_motion_batch(records: &[PerfMotion<'_>], repeats: usize) {
    for _ in 0..repeats {
        for record in records {
            core::hint::black_box(core::hint::black_box(record).run());
        }
    }
}

pub fn perf_motion_exact(records: &[PerfMotion<'_>]) -> Vec<(i32, usize)> {
    records.iter().map(PerfMotion::run).collect()
}

#[cfg(itg_capture_extra)]
mod motion_trace {
    extern crate std;
    use super::*;
    use std::{
        fs::File,
        io::{BufWriter, Write},
        sync::{
            atomic::{AtomicUsize, Ordering::Relaxed},
            Mutex,
        },
    };
    static INDEX: AtomicUsize = AtomicUsize::new(0);
    static RECORDS: Mutex<Vec<u8>> = Mutex::new(Vec::new());

    pub fn capture(bits: &Bits<'_>, predictor: i32, code: u8) {
        if INDEX.fetch_add(1, Relaxed) % 512 != 0 {
            return;
        }
        let (offset, bytes) = bits.perf_tail();
        let bytes = &bytes[..bytes.len().min(4)];
        let mut out = RECORDS.lock().unwrap();
        out.extend(predictor.to_le_bytes());
        out.extend([code, offset, bytes.len() as u8]);
        out.extend_from_slice(bytes);
    }

    pub fn write(path: &str) {
        BufWriter::new(File::create(path).unwrap())
            .write_all(&RECORDS.lock().unwrap())
            .unwrap();
    }
}

#[cfg(itg_capture_extra)]
pub use motion_trace::write as write_motion_trace;

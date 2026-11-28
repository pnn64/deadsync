// Included only in temporary copies of block.rs by the benchmark runner.
pub struct PerfIntra<'a> {
    coded: bool,
    separate_dc: bool,
    predict_ac: bool,
    quantizer: i32,
    plane: usize,
    neighbors: [Predictor; 3],
    offset: u8,
    bytes: &'a [u8],
}

impl<'a> PerfIntra<'a> {
    pub fn parse(mut data: &'a [u8]) -> alloc::vec::Vec<Self> {
        fn i32(data: &mut &[u8]) -> i32 {
            let (value, rest) = data.split_first_chunk::<4>().unwrap();
            *data = rest;
            i32::from_le_bytes(*value)
        }
        let mut records = alloc::vec::Vec::new();
        while !data.is_empty() {
            let (flags, rest) = data.split_first_chunk::<5>().unwrap();
            data = rest;
            let quantizer = i32(&mut data);
            let neighbors = core::array::from_fn(|_| Predictor {
                dc: i32(&mut data),
                vertical: core::array::from_fn(|_| i32(&mut data)),
                horizontal: core::array::from_fn(|_| i32(&mut data)),
                quantizer: i32(&mut data),
            });
            let (length, rest) = data.split_first_chunk::<2>().unwrap();
            let (bytes, rest) = rest.split_at(u16::from_le_bytes(*length) as usize);
            data = rest;
            records.push(Self {
                coded: flags[0] != 0,
                separate_dc: flags[1] != 0,
                predict_ac: flags[2] != 0,
                plane: flags[3] as usize,
                offset: flags[4],
                quantizer,
                neighbors,
                bytes,
            });
        }
        assert!(!records.is_empty());
        records
    }

    fn run(&self) -> ([i16; 64], Predictor, usize) {
        let mut bits = Bits::new(self.bytes);
        bits.skip(self.offset).unwrap();
        let (samples, predictor) = intra::<false>(
            &mut bits,
            self.coded,
            self.separate_dc,
            self.predict_ac,
            self.quantizer,
            self.plane,
            self.neighbors,
        )
        .unwrap();
        (samples, predictor, bits.perf_position())
    }

    fn dc(&self) -> (i32, usize) {
        assert!(self.separate_dc);
        let mut bits = Bits::new(self.bytes);
        bits.skip(self.offset).unwrap();
        let difference = dc_difference::<false>(&mut bits, self.plane).unwrap();
        (difference, bits.perf_position())
    }
}

pub fn perf_intra_batch(records: &[PerfIntra<'_>], repeats: usize) {
    for _ in 0..repeats {
        for record in records {
            core::hint::black_box(core::hint::black_box(record).run());
        }
    }
}

pub fn perf_intra_exact(records: &[PerfIntra<'_>]) -> alloc::vec::Vec<i32> {
    let mut result = alloc::vec::Vec::new();
    for record in records {
        let (samples, state, position) = record.run();
        result.extend(samples.map(i32::from));
        result.push(state.dc);
        result.extend(state.vertical);
        result.extend(state.horizontal);
        result.push(state.quantizer);
        result.push(position as i32);
    }
    result
}

pub fn perf_dc_batch(records: &[PerfIntra<'_>], repeats: usize) {
    for _ in 0..repeats {
        for record in records {
            core::hint::black_box(core::hint::black_box(record).dc());
        }
    }
}

pub fn perf_dc_exact(records: &[PerfIntra<'_>]) -> alloc::vec::Vec<(i32, usize)> {
    records.iter().map(PerfIntra::dc).collect()
}

#[cfg(itg_capture)]
mod trace {
    extern crate std;
    use super::*;
    use alloc::vec::Vec;
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

    pub fn capture(
        bits: &Bits<'_>,
        coded: bool,
        separate_dc: bool,
        predict_ac: bool,
        quantizer: i32,
        plane: usize,
        neighbors: [Predictor; 3],
    ) {
        if INDEX.fetch_add(1, Relaxed) % 512 != 0 {
            return;
        }
        let (offset, bytes) = bits.perf_tail();
        // A legal 64-coefficient block cannot consume 512 bytes.
        let bytes = &bytes[..bytes.len().min(512)];
        let mut out = RECORDS.lock().unwrap();
        out.extend([
            coded as u8,
            separate_dc as u8,
            predict_ac as u8,
            plane as u8,
            offset,
        ]);
        out.extend(quantizer.to_le_bytes());
        for p in neighbors {
            out.extend(p.dc.to_le_bytes());
            for value in p.vertical.into_iter().chain(p.horizontal) {
                out.extend(value.to_le_bytes());
            }
            out.extend(p.quantizer.to_le_bytes());
        }
        out.extend((bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(bytes);
    }

    pub fn write_trace(path: &str) {
        BufWriter::new(File::create(path).unwrap())
            .write_all(&RECORDS.lock().unwrap())
            .unwrap();
    }
}

#[cfg(itg_capture)]
pub use trace::write_trace;

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

pub struct PerfVectors<'a> {
    grid: core::cell::RefCell<Grid<Vector>>,
    origin: [usize; 2],
    original: [Vector; 4],
    skipped: bool,
    four: bool,
    code: u8,
    offset: u8,
    bytes: &'a [u8],
}

impl<'a> PerfVectors<'a> {
    pub fn parse(mut data: &'a [u8]) -> Vec<Self> {
        let mut records = Vec::new();
        while !data.is_empty() {
            let (header, rest) = data.split_first_chunk::<33>().unwrap();
            let number = |i: usize| {
                u32::from_le_bytes(header[i * 4..i * 4 + 4].try_into().unwrap()) as usize
            };
            let (width, height) = (number(0), number(1));
            let packet = (number(2), number(3));
            let shift = number(4) as u8;
            let origin = [number(5), number(6)];
            let (bytes, rest) = rest.split_at(header[32] as usize);
            let (cells, rest) = rest.split_at(width * height * 8);
            let cells: Vec<Vector> = cells
                .chunks_exact(8)
                .map(|p| {
                    [
                        i32::from_le_bytes(p[..4].try_into().unwrap()),
                        i32::from_le_bytes(p[4..].try_into().unwrap()),
                    ]
                })
                .collect();
            let original =
                core::array::from_fn(|i| cells[(origin[1] + i / 2) * width + origin[0] + i % 2]);
            records.push(Self {
                grid: core::cell::RefCell::new(Grid {
                    width,
                    height,
                    cells,
                    shift,
                    packet,
                }),
                origin,
                original,
                skipped: header[28] != 0,
                four: header[29] != 0,
                code: header[30],
                offset: header[31],
                bytes,
            });
            data = rest;
        }
        records
    }

    fn run(&self) -> ([Vector; 4], [Vector; 4], usize) {
        let mut grid = self.grid.borrow_mut();
        let mut bits = Bits::new(self.bytes);
        bits.skip(self.offset).unwrap();
        let mb = Macroblock {
            intra: false,
            four_vectors: self.four,
            predict_ac: false,
            separate_dc: false,
            pattern: 0,
            skipped: self.skipped,
        };
        let vectors = motion_vectors(&mut bits, &mut grid, self.origin, &mb, self.code).unwrap();
        let [x, y] = self.origin;
        let stored = core::array::from_fn(|i| grid.cells[(y + i / 2) * grid.width + x + i % 2]);
        for (i, &v) in self.original.iter().enumerate() {
            grid.set(x + i % 2, y + i / 2, v);
        }
        (vectors, stored, bits.perf_position())
    }
}

pub fn perf_vectors_batch(records: &[PerfVectors<'_>], repeats: usize) {
    for _ in 0..repeats {
        for record in records {
            core::hint::black_box(core::hint::black_box(record).run());
        }
    }
}

pub fn perf_vectors_exact(records: &[PerfVectors<'_>]) -> Vec<([Vector; 4], [Vector; 4], usize)> {
    records.iter().map(PerfVectors::run).collect()
}

#[cfg(itg_capture_vectors)]
mod vectors_trace {
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

    pub(super) fn capture(
        bits: &Bits<'_>,
        grid: &Grid<Vector>,
        x: usize,
        y: usize,
        mb: &Macroblock,
        code: u8,
    ) {
        if INDEX.fetch_add(1, Relaxed) % 2048 != 0 {
            return;
        }
        let (offset, bytes) = bits.perf_tail();
        let bytes = &bytes[..bytes.len().min(32)];
        let mut out = RECORDS.lock().unwrap();
        for n in [
            grid.width,
            grid.height,
            grid.packet.0,
            grid.packet.1,
            grid.shift as usize,
            x,
            y,
        ] {
            out.extend((n as u32).to_le_bytes());
        }
        out.extend([
            mb.skipped as u8,
            mb.four_vectors as u8,
            code,
            offset,
            bytes.len() as u8,
        ]);
        out.extend_from_slice(bytes);
        for vector in &grid.cells {
            for value in vector {
                out.extend(value.to_le_bytes());
            }
        }
    }

    pub fn write_vectors(path: &str) {
        BufWriter::new(File::create(path).unwrap())
            .write_all(&RECORDS.lock().unwrap())
            .unwrap();
    }
}

#[cfg(itg_capture_vectors)]
pub use vectors_trace::write_vectors;

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

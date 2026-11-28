// Included in temporary copies of frame.rs, never in the shipping crate.
pub struct PerfEdge {
    plane: Plane,
    origin: [i32; 2],
    span: usize,
    rows: usize,
}

impl PerfEdge {
    pub fn parse(mut data: &[u8]) -> Vec<Self> {
        let mut records = Vec::new();
        while !data.is_empty() {
            let (header, rest) = data.split_first_chunk::<32>().unwrap();
            let number =
                |i: usize| u32::from_le_bytes(header[i * 4..i * 4 + 4].try_into().unwrap());
            let (samples, rest) = rest.split_at(number(7) as usize);
            records.push(Self {
                plane: Plane {
                    width: number(0) as usize,
                    height: number(1) as usize,
                    stride: number(2) as usize,
                    samples: samples.to_vec(),
                },
                origin: [number(3) as i32, number(4) as i32],
                span: number(5) as usize,
                rows: number(6) as usize,
            });
            data = rest;
        }
        records
    }

    fn run(&self) -> [u8; 289] {
        let mut tile = [0; 289];
        self.plane
            .extend_edge(self.origin, self.span, self.rows, &mut tile);
        tile
    }
}

pub struct PerfTransform([i16; 64]);

impl PerfTransform {
    pub fn parse(data: &[u8]) -> Vec<Self> {
        assert_eq!(data.len() % 128, 0);
        data.chunks_exact(128)
            .map(|row| {
                Self(core::array::from_fn(|i| {
                    i16::from_le_bytes([row[2 * i], row[2 * i + 1]])
                }))
            })
            .collect()
    }

    fn run(&self) -> [i16; 64] {
        crate::transform::inverse(&self.0)
    }
}

pub fn perf_edge_batch(records: &[PerfEdge], repeats: usize) {
    for _ in 0..repeats {
        for record in records {
            core::hint::black_box(core::hint::black_box(record).run());
        }
    }
}

pub fn perf_edge_exact(records: &[PerfEdge]) -> Vec<[u8; 289]> {
    records.iter().map(PerfEdge::run).collect()
}

pub fn perf_transform_batch(records: &[PerfTransform], repeats: usize) {
    for _ in 0..repeats {
        for record in records {
            core::hint::black_box(core::hint::black_box(record).run());
        }
    }
}

pub fn perf_transform_exact(records: &[PerfTransform]) -> Vec<[i16; 64]> {
    records.iter().map(PerfTransform::run).collect()
}

#[cfg(itg_capture_pixels)]
mod pixel_trace {
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
    static EDGE_INDEX: AtomicUsize = AtomicUsize::new(0);
    static TRANSFORM_INDEX: AtomicUsize = AtomicUsize::new(0);
    static EDGES: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    static TRANSFORMS: Mutex<Vec<u8>> = Mutex::new(Vec::new());

    pub(super) fn edge(plane: &Plane, x: i32, y: i32, span: usize, rows: usize) {
        if EDGE_INDEX.fetch_add(1, Relaxed) % 512 != 0 {
            return;
        }
        let mut out = EDGES.lock().unwrap();
        for n in [
            plane.width as u32,
            plane.height as u32,
            plane.stride as u32,
            x as u32,
            y as u32,
            span as u32,
            rows as u32,
            plane.samples.len() as u32,
        ] {
            out.extend(n.to_le_bytes());
        }
        out.extend_from_slice(&plane.samples);
    }

    pub(crate) fn transform(coefficients: &[i16; 64]) {
        if TRANSFORM_INDEX.fetch_add(1, Relaxed) % 512 != 0 {
            return;
        }
        let mut out = TRANSFORMS.lock().unwrap();
        for value in coefficients {
            out.extend(value.to_le_bytes());
        }
    }

    pub fn write_edges(path: &str) {
        BufWriter::new(File::create(path).unwrap())
            .write_all(&EDGES.lock().unwrap())
            .unwrap();
    }

    pub fn write_transforms(path: &str) {
        BufWriter::new(File::create(path).unwrap())
            .write_all(&TRANSFORMS.lock().unwrap())
            .unwrap();
    }
}

#[cfg(itg_capture_pixels)]
pub(crate) use pixel_trace::transform as capture_transform;
#[cfg(itg_capture_pixels)]
pub use pixel_trace::{write_edges, write_transforms};

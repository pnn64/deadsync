// Included in a temporary copy of frame.rs by the performance runner.
// It is never compiled into the shipping crate.
pub type PerfBlock = (bool, [i16; 64], [u8; 64]);

pub fn perf_inverse_batch(blocks: &[[i16; 64]], repeats: usize) {
    for _ in 0..repeats {
        for block in blocks {
            core::hint::black_box(crate::transform::inverse(core::hint::black_box(block)));
        }
    }
}

pub struct PerfPlane {
    plane: Plane,
    initial: Vec<u8>,
}

impl PerfPlane {
    pub fn new(blocks: &[PerfBlock]) -> Self {
        let height = blocks.len().div_ceil(40) * 8;
        let mut plane = Plane::new(320, height, 8, height);
        for (i, (_, _, pixels)) in blocks.iter().enumerate() {
            let start = (i / 40 * 320 + i % 40) * 8;
            for (row, src) in pixels.as_chunks::<8>().0.iter().enumerate() {
                let offset = start + row * 320;
                plane.samples[offset..offset + 8].copy_from_slice(src);
            }
        }
        Self {
            initial: plane.samples.clone(),
            plane,
        }
    }

    pub fn run(&mut self, blocks: &[PerfBlock], repeats: usize) {
        for _ in 0..repeats {
            self.plane.samples.copy_from_slice(&self.initial);
            for (i, (intra, block, _)) in blocks.iter().enumerate() {
                self.plane.block(i % 40 * 8, i / 40 * 8, block, *intra);
            }
            core::hint::black_box(&self.plane.samples);
        }
    }

    pub fn samples(&self) -> &[u8] {
        &self.plane.samples
    }
}

#[cfg(itg_capture)]
mod itg_trace {
    extern crate std;
    use super::PerfBlock;
    use std::{
        fs::File,
        io::{BufWriter, Write},
        path::Path,
        sync::{
            atomic::{AtomicUsize, Ordering::Relaxed},
            Mutex,
        },
    };
    static TRANSFORM_INDEX: AtomicUsize = AtomicUsize::new(0);
    static PLANE_INDEX: AtomicUsize = AtomicUsize::new(0);
    static COEFFICIENTS: Mutex<Vec<[i16; 64]>> = Mutex::new(Vec::new());
    static BLOCKS: Mutex<Vec<PerfBlock>> = Mutex::new(Vec::new());
    use alloc::vec::Vec;

    pub fn record_transform(block: &[i16; 64]) {
        if TRANSFORM_INDEX.fetch_add(1, Relaxed) % 512 == 0 {
            COEFFICIENTS.lock().unwrap().push(*block);
        }
    }

    pub fn sample_plane() -> bool {
        PLANE_INDEX.fetch_add(1, Relaxed) % 512 == 0
    }

    pub fn record_plane(intra: bool, block: [i16; 64], pixels: [u8; 64]) {
        BLOCKS.lock().unwrap().push((intra, block, pixels));
    }

    pub fn write_trace(directory: &str) {
        let dir = Path::new(directory);
        let mut coeff = BufWriter::new(File::create(dir.join("coefficients.bin")).unwrap());
        for block in COEFFICIENTS.lock().unwrap().iter() {
            for value in block {
                coeff.write_all(&value.to_le_bytes()).unwrap();
            }
        }
        let mut pixels = BufWriter::new(File::create(dir.join("blocks.bin")).unwrap());
        for (intra, block, prediction) in BLOCKS.lock().unwrap().iter() {
            pixels.write_all(&[u8::from(*intra)]).unwrap();
            for value in block {
                pixels.write_all(&value.to_le_bytes()).unwrap();
            }
            pixels.write_all(prediction).unwrap();
        }
    }
}

#[cfg(itg_capture)]
pub use itg_trace::{record_plane, record_transform, sample_plane, write_trace};

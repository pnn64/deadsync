// Appended only to temporary frame.rs copies by the benchmark runner.
// Trace records preserve padded plane geometry and all source samples. Replay
// restores only the affected destination tile before calling Plane::predict.
struct PerfPrediction {
    source: Plane,
    destination: Plane,
    width: usize,
    x: usize,
    y: usize,
    motion: [i32; 2],
    average: bool,
    initial: [u8; 256],
}

pub struct PerfPredictions {
    records: Vec<PerfPrediction>,
}

impl PerfPredictions {
    pub fn new(mut bytes: &[u8]) -> Self {
        fn take<'a>(bytes: &mut &'a [u8], len: usize) -> &'a [u8] {
            let (head, tail) = bytes.split_at(len);
            *bytes = tail;
            head
        }
        assert_eq!(take(&mut bytes, 8), b"ITGPRED1");
        let mut records = Vec::new();
        while !bytes.is_empty() {
            // W, destination width/height/stride/len, source width/height/
            // stride/len, x, y, motion x, motion y; all little-endian u32.
            let fields: [u32; 13] = core::array::from_fn(|_| {
                u32::from_le_bytes(take(&mut bytes, 4).try_into().unwrap())
            });
            let width = fields[0] as usize;
            assert!(width == 8 || width == 16);
            let average = take(&mut bytes, 1)[0] != 0;
            let mut initial = [0; 256];
            initial[..width * width].copy_from_slice(take(&mut bytes, width * width));
            let source = Plane {
                width: fields[5] as usize,
                height: fields[6] as usize,
                stride: fields[7] as usize,
                samples: take(&mut bytes, fields[8] as usize).to_vec(),
            };
            let destination = Plane {
                width: fields[1] as usize,
                height: fields[2] as usize,
                stride: fields[3] as usize,
                samples: vec![0; fields[4] as usize],
            };
            records.push(PerfPrediction {
                source,
                destination,
                width,
                x: fields[9] as usize,
                y: fields[10] as usize,
                motion: [fields[11] as i32, fields[12] as i32],
                average,
                initial,
            });
        }
        assert!(!records.is_empty());
        Self { records }
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn samples(&self) -> impl Iterator<Item = &[u8]> {
        self.records
            .iter()
            .map(|record| record.destination.samples.as_slice())
    }

    pub fn run(&mut self, repeats: usize) {
        for _ in 0..repeats {
            for record in &mut self.records {
                for row in 0..record.width {
                    let start = (record.y + row) * record.destination.stride + record.x;
                    record.destination.samples[start..start + record.width]
                        .copy_from_slice(&record.initial[row * record.width..][..record.width]);
                }
                match record.width {
                    8 => record.destination.predict::<8>(
                        record.x,
                        record.y,
                        &record.source,
                        record.motion,
                        record.average,
                    ),
                    16 => record.destination.predict::<16>(
                        record.x,
                        record.y,
                        &record.source,
                        record.motion,
                        record.average,
                    ),
                    _ => unreachable!(),
                }
                core::hint::black_box(&record.destination.samples);
            }
        }
    }
}

#[cfg(itg_prediction_capture)]
mod itg_prediction_trace {
    extern crate std;
    use super::Plane;
    use alloc::vec::Vec;
    use std::{
        fs::File,
        io::{BufWriter, Write},
        path::Path,
        sync::{
            atomic::{AtomicUsize, Ordering::Relaxed},
            Mutex,
        },
    };

    struct Record {
        fields: [u32; 13],
        average: bool,
        initial: Vec<u8>,
        source: Vec<u8>,
    }

    static INDEX: AtomicUsize = AtomicUsize::new(0);
    static PREDICTIONS: Mutex<Vec<Record>> = Mutex::new(Vec::new());

    pub(super) fn record_prediction<const W: usize>(
        destination: &Plane,
        x: usize,
        y: usize,
        source: &Plane,
        motion: [i32; 2],
        average: bool,
    ) {
        if INDEX.fetch_add(1, Relaxed) % 16_384 != 0 {
            return;
        }
        let mut initial = Vec::with_capacity(W * W);
        for row in 0..W {
            let start = (y + row) * destination.stride + x;
            initial.extend_from_slice(&destination.samples[start..start + W]);
        }
        let fields = [
            W as u32,
            destination.width as u32,
            destination.height as u32,
            destination.stride as u32,
            destination.samples.len() as u32,
            source.width as u32,
            source.height as u32,
            source.stride as u32,
            source.samples.len() as u32,
            x as u32,
            y as u32,
            motion[0] as u32,
            motion[1] as u32,
        ];
        PREDICTIONS.lock().unwrap().push(Record {
            fields,
            average,
            initial,
            source: source.samples.clone(),
        });
    }

    pub fn write_prediction_trace(directory: &str) {
        let mut file =
            BufWriter::new(File::create(Path::new(directory).join("predictions.bin")).unwrap());
        file.write_all(b"ITGPRED1").unwrap();
        for record in PREDICTIONS.lock().unwrap().iter() {
            for value in record.fields {
                file.write_all(&value.to_le_bytes()).unwrap();
            }
            file.write_all(&[u8::from(record.average)]).unwrap();
            file.write_all(&record.initial).unwrap();
            file.write_all(&record.source).unwrap();
        }
    }
}

#[cfg(itg_prediction_capture)]
use itg_prediction_trace::record_prediction;
#[cfg(itg_prediction_capture)]
pub use itg_prediction_trace::write_prediction_trace;

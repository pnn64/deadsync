// Appended only to temporary slice.rs copies by the benchmark runner.
// Eight bytes cover both motion components and the final four-byte peek,
// including a starting bit offset and the maximum supported f_code of nine.
pub type PerfMotion = ([u8; 8], u8, u8, [u8; 2], [i32; 2]);
pub type PerfMotionValue = ([i32; 2], usize, u8);

fn perf_motion_sequence() -> Sequence {
    Sequence::parse(&[0x01, 0x00, 0x10, 0x13, 0, 0, 0x20, 0]).unwrap()
}

fn perf_motion_value(
    (bytes, len, offset, f_code, initial): &PerfMotion,
    sequence: &Sequence,
) -> PerfMotionValue {
    let coding = Coding {
        f_code: [*f_code; 2],
        full_pel: [false; 2],
        dc_precision: 0,
        concealment_vectors: false,
        nonlinear_scale: false,
        intra_vlc: false,
        alternate_scan: false,
    };
    let slice = Slice {
        sequence,
        coding: &coding,
        references: [None; 2],
    };
    let mut bits = Bits::perf_motion_from_context(&bytes[..usize::from(*len)], *offset);
    let mut motion = *initial;
    let status = match slice.read_motion(&mut bits, &mut motion, 0) {
        Ok(()) => 0,
        Err(Error::Truncated) => 1,
        Err(Error::Invalid("variable-length code")) => 2,
        Err(Error::Invalid("motion vector f_code")) => 3,
        Err(error) => panic!("unexpected motion error: {error:?}"),
    };
    (motion, bits.perf_motion_position(), status)
}

pub fn perf_motion_values(records: &[PerfMotion]) -> alloc::vec::Vec<PerfMotionValue> {
    let sequence = perf_motion_sequence();
    records
        .iter()
        .map(|record| perf_motion_value(record, &sequence))
        .collect()
}

pub fn perf_motion_batch(records: &[PerfMotion], repeats: usize) -> u64 {
    let sequence = perf_motion_sequence();
    let mut sum = 0u64;
    for _ in 0..repeats {
        for record in records {
            let (motion, position, status) =
                perf_motion_value(core::hint::black_box(record), &sequence);
            let value = (u64::from(motion[0] as u32) << 32) | u64::from(motion[1] as u32);
            sum = sum.wrapping_add(value ^ ((position as u64) << 2) ^ u64::from(status));
        }
    }
    core::hint::black_box(sum)
}

#[cfg(itg_motion_capture)]
mod itg_motion_trace {
    extern crate std;
    use super::{Bits, PerfMotion};
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
    static INDEX: AtomicUsize = AtomicUsize::new(0);
    static MOTIONS: Mutex<Vec<PerfMotion>> = Mutex::new(Vec::new());

    pub(super) fn record_motion(bits: &Bits<'_>, motion: [i32; 2], f_code: [u8; 2]) {
        if INDEX.fetch_add(1, Relaxed) % 256 != 0 {
            return;
        }
        let (bytes, len, offset) = bits.perf_motion_context();
        MOTIONS
            .lock()
            .unwrap()
            .push((bytes, len, offset, f_code, motion));
    }

    pub fn write_motion_trace(directory: &str) {
        let mut file =
            BufWriter::new(File::create(Path::new(directory).join("read-motions.bin")).unwrap());
        for (bytes, len, offset, f_code, motion) in MOTIONS.lock().unwrap().iter() {
            file.write_all(bytes).unwrap();
            file.write_all(&[*len, *offset]).unwrap();
            file.write_all(f_code).unwrap();
            for value in motion {
                file.write_all(&value.to_le_bytes()).unwrap();
            }
        }
    }
}

#[cfg(itg_motion_capture)]
use itg_motion_trace::record_motion;
#[cfg(itg_motion_capture)]
pub use itg_motion_trace::write_motion_trace;

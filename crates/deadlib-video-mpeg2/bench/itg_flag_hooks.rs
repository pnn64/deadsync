// Appended only to temporary bitstream.rs copies by the benchmark runner.
// A record keeps the real next bytes, available length capped at four, and
// original bit offset. Four-byte records preserve the normal peek fast path.
pub type PerfFlag = ([u8; 4], u8, u8);

fn perf_flag_value((bytes, len, offset): &PerfFlag) -> u64 {
    let mut bits = Bits::new(&bytes[..usize::from(*len)]);
    bits.position = usize::from(*offset);
    let status = match bits.flag() {
        Ok(false) => 0,
        Ok(true) => 1,
        Err(Error::Truncated) => 2,
        Err(error) => panic!("unexpected flag error: {error:?}"),
    };
    (bits.position as u64) << 2 | status
}

pub fn perf_flag_values(records: &[PerfFlag]) -> alloc::vec::Vec<u64> {
    records.iter().map(perf_flag_value).collect()
}

pub fn perf_flag_batch(records: &[PerfFlag], repeats: usize) -> u64 {
    let mut sum = 0u64;
    for _ in 0..repeats {
        for record in records {
            sum = sum.wrapping_add(perf_flag_value(core::hint::black_box(record)));
        }
    }
    core::hint::black_box(sum)
}

#[cfg(itg_flag_capture)]
mod itg_flag_trace {
    extern crate std;
    use super::{Bits, PerfFlag};
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
    static FLAGS: Mutex<Vec<PerfFlag>> = Mutex::new(Vec::new());

    pub(super) fn record_flag(bits: &Bits<'_>) {
        if INDEX.fetch_add(1, Relaxed) % 256 != 0 {
            return;
        }
        let tail = &bits.bytes[bits.position / 8..];
        let len = tail.len().min(4);
        let mut bytes = [0; 4];
        bytes[..len].copy_from_slice(&tail[..len]);
        FLAGS
            .lock()
            .unwrap()
            .push((bytes, len as u8, (bits.position % 8) as u8));
    }

    pub fn write_flag_trace(directory: &str) {
        let mut file =
            BufWriter::new(File::create(Path::new(directory).join("flags.bin")).unwrap());
        for (bytes, len, offset) in FLAGS.lock().unwrap().iter() {
            file.write_all(bytes).unwrap();
            file.write_all(&[*len, *offset]).unwrap();
        }
    }
}

#[cfg(itg_flag_capture)]
use itg_flag_trace::record_flag;
#[cfg(itg_flag_capture)]
pub use itg_flag_trace::write_flag_trace;

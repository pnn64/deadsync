// Included only in temporary copies of bitstream.rs.
pub struct PerfBits<'a> {
    bits: Bits<'a>,
    count: u8,
}

impl<'a> PerfBits<'a> {
    pub fn parse(mut data: &'a [u8]) -> alloc::vec::Vec<Self> {
        let mut records = alloc::vec::Vec::new();
        while !data.is_empty() {
            let (header, rest) = data.split_first_chunk::<3>().unwrap();
            let (bytes, rest) = rest.split_at(header[2] as usize);
            let mut bits = Bits::new(bytes);
            bits.skip(header[1]).unwrap();
            records.push(Self {
                bits,
                count: header[0],
            });
            data = rest;
        }
        records
    }

    fn flag(&self) -> (Option<bool>, usize) {
        let mut bits = self.bits;
        let value = bits.flag();
        (value.ok(), bits.position)
    }

    fn peek(&self) -> u32 {
        self.bits.peek(self.count)
    }

    fn skip(&self) -> (bool, usize) {
        let mut bits = self.bits;
        let result = bits.skip(self.count);
        (result.is_ok(), bits.position)
    }
}

macro_rules! bits_replay {
    ($batch:ident, $exact:ident, $op:ident, $output:ty) => {
        pub fn $batch(records: &[PerfBits<'_>], repeats: usize) {
            for _ in 0..repeats {
                for record in records {
                    core::hint::black_box(core::hint::black_box(record).$op());
                }
            }
        }

        pub fn $exact(records: &[PerfBits<'_>]) -> alloc::vec::Vec<$output> {
            records.iter().map(PerfBits::$op).collect()
        }
    };
}

bits_replay!(
    perf_flag_batch,
    perf_flag_exact,
    flag,
    (Option<bool>, usize)
);
bits_replay!(perf_peek_batch, perf_peek_exact, peek, u32);
bits_replay!(perf_skip_batch, perf_skip_exact, skip, (bool, usize));

#[cfg(itg_capture_bits)]
mod bits_trace {
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

    struct Trace {
        index: AtomicUsize,
        records: Mutex<Vec<u8>>,
    }

    impl Trace {
        const fn new() -> Self {
            Self {
                index: AtomicUsize::new(0),
                records: Mutex::new(Vec::new()),
            }
        }

        fn capture(&self, bits: &Bits<'_>, count: u8) {
            if self.index.fetch_add(1, Relaxed) % 2048 != 0 {
                return;
            }
            let (offset, bytes) = bits.perf_tail();
            // Enough input to preserve every u8-sized skip and all lookahead.
            let bytes = &bytes[..bytes.len().min(33)];
            let mut out = self.records.lock().unwrap();
            out.extend([count, offset, bytes.len() as u8]);
            out.extend_from_slice(bytes);
        }

        fn write(&self, path: &str) {
            BufWriter::new(File::create(path).unwrap())
                .write_all(&self.records.lock().unwrap())
                .unwrap();
        }
    }

    static FLAG: Trace = Trace::new();
    static PEEK: Trace = Trace::new();
    static SKIP: Trace = Trace::new();

    pub(super) fn flag(bits: &Bits<'_>) {
        FLAG.capture(bits, 1);
    }
    pub(super) fn peek(bits: &Bits<'_>, count: u8) {
        PEEK.capture(bits, count);
    }
    pub(super) fn skip(bits: &Bits<'_>, count: u8) {
        SKIP.capture(bits, count);
    }

    pub fn write_bits(path: &str) {
        FLAG.write(&alloc::format!("{path}.flag"));
        PEEK.write(&alloc::format!("{path}.peek"));
        SKIP.write(&alloc::format!("{path}.skip"));
    }
}

#[cfg(itg_capture_bits)]
pub use bits_trace::write_bits;

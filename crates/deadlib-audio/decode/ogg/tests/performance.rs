//! Release-only CPU/allocation measurements; fixture construction is not timed.
//!
//! Run `cargo test -p deadlib-audio-decode-ogg --release --test performance --
//! --ignored --nocapture --test-threads=1`. `OGG_BENCH_FILTER` selects comma-separated
//! label substrings; `OGG_BENCH_CPU` selects the Windows logical processor.
//! Windows reports executed thread cycles; other platforms report zero cycles.
use deadlib_audio_decode_ogg::*;
use std::{hint::black_box, ops::ControlFlow, time::Instant};

#[path = "../../../../../tests/support/allocations.rs"]
#[allow(dead_code)]
mod allocations;

#[cfg(windows)]
fn cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }
    let mut cycles = 0;
    // SAFETY: the current-thread pseudo-handle is valid and the output is writable.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) },
        0
    );
    cycles
}

#[cfg(not(windows))]
fn cycles() -> u64 {
    0
}

fn measure(label: &str, iterations: usize, bytes: usize, mut work: impl FnMut()) {
    let filter = std::env::var("OGG_BENCH_FILTER").unwrap_or_default();
    if !filter.is_empty() && !filter.split(',').any(|filter| label.contains(filter)) {
        return;
    }
    for _ in 0..32 {
        work();
    }
    let (_, churn) = allocations::measure(&mut work);
    println!(
        "alloc,{label},{},{},{}",
        churn.allocs, churn.reallocs, churn.allocated_bytes
    );
    for round in 0..9 {
        let start_cycles = cycles();
        let start = Instant::now();
        for _ in 0..iterations {
            work();
        }
        let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
        let cpu = (cycles() - start_cycles) as f64 / iterations as f64;
        println!("sample,{label},{round},{iterations},{bytes},{cpu:.3},{ns:.3}");
    }
}

fn stream(packet_size: usize, packets: usize, page_body: usize) -> Vec<u8> {
    let data: Vec<_> = (0..packet_size).map(|i| (i * 37) as u8).collect();
    let mut writer = PacketWriter::with_options(
        Vec::new(),
        7,
        WriteOptions {
            target_page_body: page_body,
        },
    );
    writer.write_packet(b"header", 0, false).unwrap();
    for number in 1..=packets {
        writer
            .write_packet(&data, number as u64, number == packets)
            .unwrap();
    }
    writer.into_inner()
}

fn pages(bytes: &[u8]) -> Vec<(u64, Page<'_>)> {
    let mut offset = 0;
    let mut pages = Vec::new();
    while offset < bytes.len() {
        let (page, length) = Page::parse(&bytes[offset..]).unwrap();
        pages.push((offset as u64, page));
        offset += length;
    }
    pages
}

#[derive(Default)]
struct ObservedWriter(usize);

impl std::io::Write for ObservedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        black_box(bytes);
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn demux_reset_reuses_capacity_and_restarts_packet_numbering() {
    let bytes = stream(300, 100, 8192);
    let parsed = pages(&bytes);
    let mut demux = Demuxer::default();
    let mut decode = || {
        demux.reset();
        let mut number = 0;
        for &(offset, page) in &parsed {
            demux
                .push_page(page, offset, |packet| {
                    assert_eq!(packet.info.number, number);
                    assert_eq!(packet.info.chain, 0);
                    number += 1;
                })
                .unwrap();
        }
        assert_eq!(number, 101);
        demux.finish().unwrap();
    };
    decode();
    allocations::assert_no_churn(decode);
}

#[test]
fn sync_reader_uses_one_buffer_and_does_not_expose_read_scratch() {
    let bytes = stream(24, 32, 8192);
    let parsed = pages(&bytes);
    let (_, churn) = allocations::measure(|| {
        let mut reader = SyncReader::new(bytes.as_slice());
        for &(offset, expected) in &parsed {
            let SyncRead::Page(actual) = reader.next_page(SyncBoundary::Unbounded).unwrap() else {
                panic!("missing expected page");
            };
            assert_eq!(actual.offset, offset);
            assert_eq!(actual.page.bytes(), expected.bytes());
        }
        for _ in 0..2 {
            assert!(matches!(
                reader.next_page(SyncBoundary::Unbounded).unwrap(),
                SyncRead::Eof
            ));
            assert_eq!(reader.buffered_bytes(), 0);
            assert!(reader.current_page().is_none());
        }
    });
    assert_eq!(churn.allocs, 1);
    assert_eq!(churn.reallocs, 0);
}

#[test]
fn sync_read_error_cannot_publish_written_scratch() {
    struct FailingRead<'a> {
        bytes: &'a [u8],
        calls: usize,
    }
    impl std::io::Read for FailingRead<'_> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            self.calls += 1;
            match self.calls {
                1 => std::io::Read::read(&mut self.bytes, &mut out[..3]),
                2 => {
                    out.fill(0xee);
                    Err(std::io::Error::other("injected read failure"))
                }
                _ => std::io::Read::read(&mut self.bytes, out),
            }
        }
    }
    let bytes = stream(24, 32, 8192);
    let mut reader = SyncReader::new(FailingRead {
        bytes: &bytes,
        calls: 0,
    });
    assert!(matches!(
        reader.next_page(SyncBoundary::Unbounded),
        Err(Error::Io(_))
    ));
    assert_eq!(reader.position(), 0);
    assert_eq!(reader.buffered_bytes(), 3);
    for (offset, expected) in pages(&bytes) {
        let SyncRead::Page(actual) = reader.next_page(SyncBoundary::Unbounded).unwrap() else {
            panic!("read retry lost a buffered page");
        };
        assert_eq!(actual.offset, offset);
        assert_eq!(actual.page.bytes(), expected.bytes());
    }
    assert!(matches!(
        reader.next_page(SyncBoundary::Unbounded).unwrap(),
        SyncRead::Eof
    ));
}

#[test]
fn writer_reuses_one_page_allocation_and_keeps_empty_packets_allocation_free() {
    let (_, churn) = allocations::measure(|| {
        let mut writer = PacketWriter::new(ObservedWriter::default(), 7);
        writer.write_packet(b"header", 0, false).unwrap();
        for number in 1..=256 {
            writer
                .write_packet(&[37; 300], number, number == 256)
                .unwrap();
        }
    });
    assert_eq!(churn.allocs, 1);
    assert_eq!(churn.reallocs, 0);
    allocations::assert_no_churn(|| {
        let mut writer = PacketWriter::new(ObservedWriter::default(), 7);
        for number in 0..512 {
            writer.write_packet(&[], number, number == 511).unwrap();
        }
    });
}

#[test]
#[ignore = "CPU/allocation benchmark; run in release with --ignored --nocapture --test-threads=1"]
// Debug builds must compile this ignored test, but must not run measurements.
#[allow(clippy::assertions_on_constants)]
fn benchmark_container() {
    assert!(!cfg!(debug_assertions), "benchmarks require --release");
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut std::ffi::c_void;
            fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
        }
        let cpu: u32 = std::env::var("OGG_BENCH_CPU")
            .map(|value| value.parse().expect("OGG_BENCH_CPU must be an integer"))
            .unwrap_or(2);
        let mask = 1usize
            .checked_shl(cpu)
            .expect("benchmark CPU exceeds affinity mask");
        // SAFETY: only changes this benchmark thread's affinity to the requested CPU.
        assert_ne!(
            unsafe { SetThreadAffinityMask(GetCurrentThread(), mask) },
            0
        );
    }
    println!("\nkind,label,round,iterations,bytes,thread_cycles,wall_ns");
    for (label, size, count, target) in [
        ("small", 24, 32, 8192),
        ("audio", 300, 256, 8192),
        ("dense", 1, 1020, 8192),
        ("spanning", 200_000, 4, 8192),
    ] {
        let bytes = stream(size, count, target);
        let parsed = pages(&bytes);
        let iterations = if label == "spanning" { 300 } else { 30_000 };
        measure(
            &format!("fragments/{label}"),
            iterations,
            bytes.len(),
            || {
                for &(_, page) in &parsed {
                    for fragment in black_box(page).fragments() {
                        black_box(fragment);
                    }
                }
            },
        );
        let mut demux = Demuxer::default();
        measure(&format!("demux/{label}"), iterations, bytes.len(), || {
            demux.reset();
            for &(offset, page) in &parsed {
                demux
                    .push_page(black_box(page), offset, |packet| {
                        black_box(packet);
                    })
                    .unwrap();
            }
            demux.finish().unwrap();
        });
        let mut capture = PacketCapture::new(7, Limits::default());
        measure(&format!("capture/{label}"), iterations, bytes.len(), || {
            capture.reset();
            for &(offset, page) in &parsed {
                capture
                    .push_page(black_box(page), offset, |packet| {
                        black_box(packet);
                    })
                    .unwrap();
            }
            capture.finish().unwrap();
        });
        measure(&format!("paused/{label}"), iterations, bytes.len(), || {
            capture.reset();
            for &(offset, page) in &parsed {
                while !capture
                    .push_page_until(black_box(page), offset, |packet| {
                        black_box(packet);
                        ControlFlow::Break(())
                    })
                    .unwrap()
                {}
            }
            capture.finish().unwrap();
        });
        measure(&format!("reader/{label}"), iterations, bytes.len(), || {
            let mut reader = PageReader::new(black_box(bytes.as_slice()));
            while let Some(page) = reader.next_page().unwrap() {
                black_box(page);
            }
        });
        measure(&format!("sync/{label}"), iterations, bytes.len(), || {
            let mut reader = SyncReader::new(black_box(bytes.as_slice()));
            while let SyncRead::Page(page) = reader.next_page(SyncBoundary::Unbounded).unwrap() {
                black_box(page);
            }
        });
        measure(&format!("writer/{label}"), iterations, size * count, || {
            let data = black_box(&bytes[..size.min(bytes.len())]);
            let mut writer = PacketWriter::new(ObservedWriter::default(), 7);
            writer.write_packet(b"header", 0, false).unwrap();
            for number in 1..=count {
                writer
                    .write_packet(data, number as u64, number == count)
                    .unwrap();
            }
            black_box(writer.into_inner().0);
        });
    }
    for size in [128, 1024, 8192, 65025] {
        let data: Vec<_> = (0..size).map(|i| (i * 37) as u8).collect();
        measure(&format!("crc/{size}"), 100_000, size, || {
            black_box(crc32(black_box(&data)));
        });
    }
}

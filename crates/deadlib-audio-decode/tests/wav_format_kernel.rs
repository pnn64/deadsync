//! WAV format parsing, including consumption and failure ordering.
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

#[allow(dead_code)]
mod wav {
    include!("../src/wav.rs");

    mod format_kernel {
        use super::*;
        use crate::{bench, perf};
        use std::hint::black_box;

        include!("support/wav_format_original.rs");

        struct Fixture(std::path::PathBuf);

        impl Fixture {
            fn new(bytes: &[u8]) -> Self {
                let path = std::env::temp_dir().join(format!(
                    "deadsync-format-{:010}-{:?}.bin",
                    std::process::id(),
                    std::thread::current().id(),
                ));
                std::fs::write(&path, bytes).unwrap();
                Self(path)
            }

            fn reader(&self) -> BufReader<File> {
                BufReader::new(File::open(&self.0).unwrap())
            }
        }

        impl Drop for Fixture {
            fn drop(&mut self) {
                std::fs::remove_file(&self.0).unwrap();
            }
        }

        fn format(extensible: bool, size: usize) -> Vec<u8> {
            let mut bytes = vec![0x97; size.max(40)];
            bytes[..16]
                .copy_from_slice(&[1, 0, 2, 0, 0x80, 0xbb, 0, 0, 0, 0xee, 2, 0, 4, 0, 16, 0]);
            if extensible {
                bytes[..2].copy_from_slice(&WAVE_FORMAT_EXTENSIBLE.to_le_bytes());
                bytes[16..20].copy_from_slice(&[22, 0, 16, 0]);
                bytes[24..26].copy_from_slice(&WAVE_FORMAT_PCM.to_le_bytes());
                bytes[26..40].copy_from_slice(&SUBFORMAT_TAIL);
            }
            bytes.truncate(size);
            bytes
        }

        fn outcome(
            result: Result<Spec, Box<dyn std::error::Error + Send + Sync>>,
        ) -> Result<(usize, u32, usize, usize), String> {
            result
                .map(|s| {
                    (
                        s.channels,
                        s.sample_rate_hz,
                        s.block_align,
                        s.encoding as usize,
                    )
                })
                .map_err(|e| e.to_string())
        }

        #[test]
        fn format_parser_preserves_errors_and_consumption() {
            for extensible in [false, true] {
                for size in [0, 15, 16, 17, 18, 39, 40, 41, 4096, 8193, 16385] {
                    let mut bytes = format(extensible, size);
                    for invalid in [false, true] {
                        if invalid && bytes.len() >= 2 {
                            bytes[..2].copy_from_slice(&0x1234u16.to_le_bytes());
                        }
                        for available in [0, size / 2, size.saturating_sub(1), size, size + 7] {
                            let mut data = bytes.clone();
                            data.resize(available, 0x73);
                            let fixture = Fixture::new(&data);
                            let mut old = fixture.reader();
                            let mut new = fixture.reader();
                            assert_eq!(
                                outcome(parse_format_chunk(&mut new, size as u32)),
                                outcome(original_parse_format_chunk(&mut old, size as u32)),
                                "extensible={extensible}, size={size}, available={available}"
                            );
                            assert_eq!(
                                new.stream_position().unwrap(),
                                old.stream_position().unwrap()
                            );
                            let mut a = Vec::new();
                            let mut b = Vec::new();
                            old.read_to_end(&mut a).unwrap();
                            new.read_to_end(&mut b).unwrap();
                            assert_eq!(a, b);
                        }
                    }
                }
            }
        }

        #[test]
        fn format_parser_preserves_field_validation() {
            for tag in [
                WAVE_FORMAT_PCM,
                WAVE_FORMAT_IEEE_FLOAT,
                WAVE_FORMAT_EXTENSIBLE,
                0x1234,
            ] {
                for (offset, values) in [
                    (2, &[0u16, 1, 2, u16::MAX][..]),
                    (4, &[0, 48000][..]),
                    (12, &[0, 1, 2, 3, 4, 6, 8, 16][..]),
                    (14, &[0, 8, 16, 24, 32, 64][..]),
                    (16, &[0, 21, 22, 23, u16::MAX][..]),
                    (18, &[0, 1, 16, 17, 32][..]),
                    (
                        24,
                        &[0, WAVE_FORMAT_PCM, WAVE_FORMAT_IEEE_FLOAT, 0x1234][..],
                    ),
                    (26, &[0, 1, u16::MAX][..]),
                ] {
                    for value in values {
                        let mut bytes = format(tag == WAVE_FORMAT_EXTENSIBLE, 40);
                        bytes[..2].copy_from_slice(&tag.to_le_bytes());
                        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
                        let fixture = Fixture::new(&bytes);
                        let mut old = fixture.reader();
                        let mut new = fixture.reader();
                        assert_eq!(
                            outcome(parse_format_chunk(&mut new, 40)),
                            outcome(original_parse_format_chunk(&mut old, 40)),
                            "tag={tag}, offset={offset}, value={value}"
                        );
                        assert_eq!(
                            new.stream_position().unwrap(),
                            old.stream_position().unwrap()
                        );
                    }
                }
            }
        }

        #[test]
        #[ignore = "release CPU and allocation benchmark"]
        fn benchmark_wav_format() {
            for buffered in [false, true] {
                for (extensible, size) in [(false, 16), (true, 40), (false, 4096), (true, 16385)] {
                    let bytes = format(extensible, size);
                    let fixture = Fixture::new(&bytes);
                    let mut reader =
                        BufReader::with_capacity(size.max(8192), File::open(&fixture.0).unwrap());
                    reader.fill_buf().unwrap();
                    let label =
                        format!("format/buffered={buffered}/extensible={extensible}/bytes={size}");
                    let parse = black_box(
                        parse_format_chunk
                            as fn(
                                &mut BufReader<File>,
                                u32,
                            )
                                -> Result<Spec, Box<dyn std::error::Error + Send + Sync>>,
                    );
                    let mut work = || {
                        if !buffered {
                            reader.seek(SeekFrom::Start(0)).unwrap();
                        }
                        black_box(parse(black_box(&mut reader), black_box(size as u32)).unwrap());
                        if buffered {
                            // Rewind inside the existing buffer without a file syscall.
                            reader.seek_relative(-(size as i64)).unwrap();
                        }
                    };
                    let (_, churn) = perf::measure(&mut work);
                    println!("{label}: allocation {churn:?}");
                    bench::measure(&label, if buffered { 50_000 } else { 2000 }, &mut work);
                }
            }
        }
    }
}

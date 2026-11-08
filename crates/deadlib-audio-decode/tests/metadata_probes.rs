pub use deadlib_audio_decode::{DecodeOptions, Reader, file_length_seconds, open_file};
use std::path::{Path, PathBuf};

mod resample {
    pub use deadlib_audio_decode::resample::*;

    pub(crate) fn take_cleared_i16(slot: &mut Option<Vec<i16>>) -> Vec<i16> {
        let mut buffer = slot.take().unwrap_or_default();
        buffer.clear();
        buffer
    }
}

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

struct Fixture(PathBuf);

impl Fixture {
    fn new(bytes: &[u8], extension: &str) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "decode-probe-{}-{}.{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            extension,
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[allow(dead_code)]
mod mp3 {
    include!("../src/mp3.rs");

    mod probes {
        use super::*;
        use crate::{Fixture, bench, perf};
        use std::hint::black_box;

        fn reference(bytes: &[u8]) -> u64 {
            for offset in 0..bytes.len().saturating_sub(4) {
                let Some(header) = parse_mpeg3_header(&bytes[offset..]) else {
                    continue;
                };
                let next_offset = offset.saturating_add(header.frame_bytes);
                let Some(next) = bytes.get(next_offset..).and_then(parse_mpeg3_header) else {
                    continue;
                };
                if !compatible_frame(header, next) {
                    continue;
                }
                let info_offset = offset
                    .saturating_add(4)
                    .saturating_add(header.side_info_bytes);
                return if bytes.get(info_offset..info_offset.saturating_add(4)) == Some(b"Info") {
                    header.frame_samples
                } else {
                    0
                };
            }
            0
        }

        fn bytes(noise: usize, kind: &str) -> Vec<u8> {
            let mut bytes = vec![if kind == "false-sync" { 0xff } else { 0x31 }; noise];
            if kind != "none" {
                let mut frame = vec![0; 417];
                frame[..4].copy_from_slice(&[0xff, 0xfb, 0x90, 0]);
                frame[36..40].copy_from_slice(if kind == "xing" { b"Xing" } else { b"Info" });
                bytes.extend_from_slice(&frame);
                bytes.extend_from_slice(&frame);
            }
            bytes
        }

        #[test]
        fn scan_preserves_offsets_false_syncs_truncation_and_first_compatible_pair() {
            for noise in 0..64 {
                for kind in ["info", "xing", "none", "false-sync"] {
                    let bytes = bytes(noise, kind);
                    for end in [0, 1, 3, 4, 5, 40, 417, bytes.len()] {
                        let input = &bytes[..end.min(bytes.len())];
                        assert_eq!(info_frames_in_prefix(input), reference(input));
                    }
                }
            }
            for first in [0, 0xfe, 0xff] {
                for second in 0..=u8::MAX {
                    for third in [0, 1, 0x90, 0xff] {
                        let header = [first, second, third, 0];
                        let mut input = if let Some(parsed) = parse_mpeg3_header(&header) {
                            let mut frame = vec![0; parsed.frame_bytes];
                            frame[..4].copy_from_slice(&header);
                            let info = 4 + parsed.side_info_bytes;
                            frame[info..info + 4].copy_from_slice(b"Info");
                            [frame.as_slice(), frame.as_slice()].concat()
                        } else {
                            header.to_vec()
                        };
                        input.extend_from_slice(&[0; 8]);
                        assert_eq!(
                            info_frames_in_prefix(&input),
                            reference(&input),
                            "header={header:02x?}"
                        );
                    }
                }
            }
            let mut seed = 0x1234_5678u32;
            for case in 0..512 {
                let mut random: Vec<_> = (0..case * 17)
                    .map(|_| {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        seed as u8
                    })
                    .collect();
                if case % 2 == 0 {
                    random.extend(bytes(0, "info"));
                }
                assert_eq!(
                    info_frames_in_prefix(&random),
                    reference(&random),
                    "case={case}"
                );
            }
        }

        #[test]
        #[ignore = "release CPU and allocation benchmark"]
        fn benchmark_mp3_scan() {
            for noise in [0, 64, 4096, 24000] {
                for kind in ["info", "xing", "none", "false-sync"] {
                    let input = bytes(noise, kind);
                    assert_eq!(info_frames_in_prefix(&input), reference(&input));
                    let label = format!("probe/mp3/noise={noise}/kind={kind}");
                    let mut work = || {
                        let scan: fn(&[u8]) -> u64 = black_box(info_frames_in_prefix);
                        black_box(scan(black_box(&input)));
                    };
                    let (_, churn) = perf::measure(work);
                    println!("{label}: allocation {churn:?}");
                    bench::measure(
                        &label,
                        (4_000_000 / input.len().max(1)).clamp(50, 100_000),
                        &mut work,
                    );
                }
            }
        }

        #[test]
        #[ignore = "release CPU and allocation benchmark"]
        fn benchmark_mp3_file_probe() {
            for noise in [0, 4096, 24000] {
                let fixture = Fixture::new(&bytes(noise, "info"), "mp3");
                for enabled in [false, true] {
                    let options = DecodeOptions {
                        mp3_gapless: false,
                        mp3_info_silence: enabled,
                    };
                    let label = format!("probe/mp3-file/noise={noise}/enabled={enabled}");
                    let mut work = || {
                        black_box(
                            info_lead_frames(black_box(fixture.path()), black_box(options))
                                .unwrap(),
                        );
                    };
                    let (_, churn) = perf::measure(work);
                    println!("{label}: allocation {churn:?}");
                    bench::measure(&label, 1000, &mut work);
                }
            }
        }
    }
}

#[allow(dead_code)]
mod wav {
    include!("../src/wav.rs");

    mod probes {
        use super::*;
        use crate::{Fixture, bench, perf};
        use std::hint::black_box;

        fn bytes(junk: usize, reversed: bool) -> Vec<u8> {
            let mut bytes = b"RIFF\0\0\0\0WAVE".to_vec();
            let mut format = b"fmt \x10\0\0\0\x01\0\x02\0".to_vec();
            format.extend_from_slice(&48000u32.to_le_bytes());
            format.extend_from_slice(&192000u32.to_le_bytes());
            format.extend_from_slice(&4u16.to_le_bytes());
            format.extend_from_slice(&16u16.to_le_bytes());
            let mut data = b"data".to_vec();
            data.extend_from_slice(&16384u32.to_le_bytes());
            data.resize(8 + 16384, 0);
            if junk > 0 {
                bytes.extend_from_slice(b"JUNK");
                bytes.extend_from_slice(&(junk as u32).to_le_bytes());
                bytes.resize(bytes.len() + junk + (junk & 1), 0x32);
            }
            if reversed {
                bytes.extend(data);
                bytes.extend(format);
            } else {
                bytes.extend(format);
                bytes.extend(data);
            }
            bytes
        }

        // Frozen chunk walk from c4c6ffd11; helpers outside the optimized walk
        // retain the production format validation and error strings.
        fn reference(
            reader: &mut BufReader<File>,
        ) -> Result<Spec, Box<dyn std::error::Error + Send + Sync>> {
            let mut riff = [0u8; 12];
            reader.read_exact(&mut riff)?;
            if &riff[..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
                return Err("unsupported WAV container (expected RIFF/WAVE)".into());
            }
            let mut format = None;
            let mut data = None;
            loop {
                let Some((chunk_id, chunk_size)) = read_chunk_header(reader)? else {
                    break;
                };
                let payload_offset = reader.stream_position()?;
                match &chunk_id {
                    b"fmt " => format = Some(parse_format_chunk(reader, chunk_size)?),
                    b"data" => data = Some((payload_offset, u64::from(chunk_size))),
                    _ => {}
                }
                let end = payload_offset
                    .saturating_add(u64::from(chunk_size))
                    .saturating_add(u64::from(chunk_size & 1));
                reader.seek(SeekFrom::Start(end))?;
                if format.is_some() && data.is_some() {
                    break;
                }
            }
            let format = format.ok_or("WAV file is missing a fmt chunk")?;
            let (data_offset, data_len) = data.ok_or("WAV file is missing a data chunk")?;
            if format.block_align == 0 {
                return Err("WAV block alignment is invalid (0)".into());
            }
            let frames_total = data_len / format.block_align as u64;
            reader.seek(SeekFrom::Start(data_offset))?;
            Ok(Spec {
                data_offset,
                frames_total,
                ..format
            })
        }

        fn describe(
            result: Result<Spec, Box<dyn std::error::Error + Send + Sync>>,
        ) -> Result<(usize, u32, usize, usize, u64, u64), String> {
            result
                .map(|spec| {
                    (
                        spec.channels,
                        spec.sample_rate_hz,
                        spec.block_align,
                        spec.encoding as usize,
                        spec.data_offset,
                        spec.frames_total,
                    )
                })
                .map_err(|error| error.to_string())
        }

        #[test]
        fn chunk_walk_preserves_spec_errors_positions_padding_and_reversed_chunks() {
            for junk in [0, 17, 4096, 65537] {
                for reversed in [false, true] {
                    let input = bytes(junk, reversed);
                    for end in [
                        0,
                        1,
                        11,
                        12,
                        19,
                        20,
                        35,
                        36,
                        43,
                        44,
                        45,
                        input.len() / 2,
                        input.len() - 1,
                        input.len(),
                    ] {
                        for origin in [0, 7] {
                            let mut prefixed = vec![0x55; origin];
                            prefixed.extend_from_slice(&input[..end.min(input.len())]);
                            let fixture = Fixture::new(&prefixed, "wav");
                            for capacity in [1, 32, 8192] {
                                let mut old = BufReader::with_capacity(
                                    capacity,
                                    File::open(fixture.path()).unwrap(),
                                );
                                let mut new = BufReader::with_capacity(
                                    capacity,
                                    File::open(fixture.path()).unwrap(),
                                );
                                old.seek(SeekFrom::Start(origin as u64)).unwrap();
                                new.seek(SeekFrom::Start(origin as u64)).unwrap();
                                assert_eq!(
                                    describe(parse_spec(&mut new)),
                                    describe(reference(&mut old)),
                                    "junk={junk}, reversed={reversed}, end={end}, origin={origin}, cap={capacity}"
                                );
                                assert_eq!(
                                    new.stream_position().unwrap(),
                                    old.stream_position().unwrap()
                                );
                                let (mut a, mut b) = (Vec::new(), Vec::new());
                                old.read_to_end(&mut a).unwrap();
                                new.read_to_end(&mut b).unwrap();
                                assert_eq!(a, b);
                            }
                        }
                    }
                }
            }
        }

        #[test]
        #[ignore = "release CPU and allocation benchmark"]
        fn benchmark_wav_walk() {
            for junk in [0, 17, 4096, 65537] {
                for reversed in [false, true] {
                    let fixture = Fixture::new(&bytes(junk, reversed), "wav");
                    let mut reader = BufReader::new(File::open(fixture.path()).unwrap());
                    let label = format!("probe/wav-walk/junk={junk}/reversed={reversed}");
                    let mut work = || {
                        reader.seek(SeekFrom::Start(0)).unwrap();
                        black_box(parse_spec(black_box(&mut reader)).unwrap());
                    };
                    let (_, churn) = perf::measure(&mut work);
                    println!("{label}: allocation {churn:?}");
                    bench::measure(&label, 2000, &mut work);
                }
            }
        }

        #[test]
        #[ignore = "release CPU and allocation benchmark"]
        fn benchmark_wav_length() {
            for junk in [0, 17, 4096, 65537] {
                let fixture = Fixture::new(&bytes(junk, false), "wav");
                let label = format!("probe/wav-length/junk={junk}");
                let mut work = || {
                    black_box(file_length_seconds(black_box(fixture.path())).unwrap());
                };
                let (_, churn) = perf::measure(work);
                println!("{label}: allocation {churn:?}");
                bench::measure(&label, 1000, &mut work);
            }
        }
    }
}

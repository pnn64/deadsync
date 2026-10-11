use super::*;
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

#[allow(dead_code)]
#[path = "wav_original.rs"]
mod original;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

struct Fixture(PathBuf);

impl Fixture {
    fn new(tag: u16, sample_bytes: usize, channels: usize, frames: usize) -> Self {
        static SERIAL: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "deadsync-wav-perf-{}-{}.wav",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let align = sample_bytes * channels;
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&(36 + frames as u32 * align as u32).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt \x10\0\0\0");
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&(channels as u16).to_le_bytes());
        bytes.extend_from_slice(&48000u32.to_le_bytes());
        bytes.extend_from_slice(&(48000 * align as u32).to_le_bytes());
        bytes.extend_from_slice(&(align as u16).to_le_bytes());
        bytes.extend_from_slice(&(8 * sample_bytes as u16).to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(frames as u32 * align as u32).to_le_bytes());
        for i in 0..frames * channels {
            if tag == WAVE_FORMAT_IEEE_FLOAT {
                let value = [0.0, -0.0, 0.25, -0.9, 1.5, -4.0, f64::NAN, f64::INFINITY][i % 8];
                if sample_bytes == 4 {
                    bytes.extend_from_slice(&(value as f32).to_le_bytes());
                } else {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
            } else {
                bytes.extend_from_slice(
                    &(i as u32).wrapping_mul(0x7fa9_539d).to_le_bytes()[..sample_bytes],
                );
            }
        }
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).unwrap();
    }
}

fn compare_packet(
    old: &mut original::Reader,
    current: &mut Reader,
    a: &mut Vec<i16>,
    b: &mut Vec<i16>,
) -> bool {
    let expected = old.read_dec_packet_into(a).map_err(|e| e.to_string());
    let actual = current.read_dec_packet_into(b).map_err(|e| e.to_string());
    assert_eq!(expected, actual);
    assert_eq!(a, b);
    assert_eq!(old.current_frame(), current.current_frame());
    expected == Ok(true)
}

#[test]
fn buffered_wav_preserves_all_encodings_packet_boundaries_and_seeks() {
    for (tag, bytes) in [(1, 1), (1, 2), (1, 3), (1, 4), (3, 4), (3, 8)] {
        for channels in [1, 2, 3] {
            for frames in [1, 4095, 4096, 4097, 9001] {
                let fixture = Fixture::new(tag, bytes, channels, frames);
                let old = original::open_file(&fixture.0).unwrap();
                let current = open_file(&fixture.0).unwrap();
                assert_eq!(
                    (old.channels, old.sample_rate_hz, old.frames_total_hint),
                    (
                        current.channels,
                        current.sample_rate_hz,
                        current.frames_total_hint
                    )
                );
                let (mut old, mut current) = (old.reader, current.reader);
                assert!(current.reader.capacity() <= old.storage_bytes());
                let (mut a, mut b) = (vec![17; 33], vec![17; 33]);
                while compare_packet(&mut old, &mut current, &mut a, &mut b) {}
                for frame in [0, 17, 4095, 4096, 8999, 10000] {
                    old.seek_frame(frame).unwrap();
                    current.seek_frame(frame).unwrap();
                    while compare_packet(&mut old, &mut current, &mut a, &mut b) {}
                }
                assert_eq!(
                    current.packet_buf.capacity(),
                    0,
                    "regular files need no packet copy"
                );
            }
        }
    }
}

#[test]
fn buffered_wav_preserves_truncated_and_misaligned_packet_errors() {
    let empty = Fixture::new(1, 2, 2, 0);
    assert_eq!(
        original::open_file(&empty.0).err().unwrap().to_string(),
        open_file(&empty.0).err().unwrap().to_string()
    );
    for misaligned in [false, true] {
        let fixture = Fixture::new(1, 2, 2, 9002);
        let mut bytes = std::fs::read(&fixture.0).unwrap();
        if misaligned {
            bytes[32..34].copy_from_slice(&5u16.to_le_bytes());
        } else {
            bytes.truncate(44 + 4096 * 4 + 31);
        }
        std::fs::write(&fixture.0, bytes).unwrap();
        let mut old = original::open_file(&fixture.0).unwrap().reader;
        let mut current = open_file(&fixture.0).unwrap().reader;
        let (mut a, mut b) = (vec![123; 17], vec![123; 17]);
        assert!(compare_packet(&mut old, &mut current, &mut a, &mut b));
        if misaligned {
            let samples = a.clone();
            let frame = old.current_frame();
            assert_eq!(
                old.read_dec_packet_into(&mut a).unwrap_err().to_string(),
                "WAV packet ended mid-sample"
            );
            assert_eq!(
                current
                    .read_dec_packet_into(&mut b)
                    .unwrap_err()
                    .to_string(),
                "WAV packet ended mid-sample"
            );
            assert_eq!(a, samples);
            assert_eq!(b, samples);
            assert_eq!(old.current_frame(), frame);
            assert_eq!(current.current_frame(), frame);
        }
        for _ in 0..4 {
            compare_packet(&mut old, &mut current, &mut a, &mut b);
        }
        for frame in [0, 4096, 9000] {
            old.seek_frame(frame).unwrap();
            current.seek_frame(frame).unwrap();
            for _ in 0..3 {
                compare_packet(&mut old, &mut current, &mut a, &mut b);
            }
        }
    }
}

#[test]
#[ignore = "paired release throughput and retained-buffer benchmark"]
fn benchmark_buffered_wav() {
    for (name, tag, bytes, channels) in [
        ("PCM8 mono", 1, 1, 1),
        ("PCM16 stereo", 1, 2, 2),
        ("Float32 stereo", 3, 4, 2),
    ] {
        let fixture = Fixture::new(tag, bytes, channels, 262144);
        let mut old = original::open_file(&fixture.0).unwrap().reader;
        let mut current = open_file(&fixture.0).unwrap().reader;
        println!(
            "{name} retained input buffers: original {} bytes / 2 allocations, current {} bytes / 1 allocation",
            old.storage_bytes(),
            current.reader.capacity() + current.packet_buf.capacity()
        );
        let (mut a, mut b) = (Vec::new(), Vec::new());
        paired_bench::compare(&format!("WAV {name}, 262144 frames"), 400, |new| {
            if new {
                current.seek_frame(0).unwrap();
                while current.read_dec_packet_into(&mut b).unwrap() {
                    black_box(&b);
                }
            } else {
                old.seek_frame(0).unwrap();
                while old.read_dec_packet_into(&mut a).unwrap() {
                    black_box(&a);
                }
            }
        });
    }
}

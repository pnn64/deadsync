//! Resident file-reader regression: flat data and cached wavl block/silence reads.
use deadlib_audio_decode_wav::{
    Adpcm, AdpcmKind, AdpcmReader, AdpcmSpec, Endian, Format, Muxer, WavlMuxer, WriteOptions,
};
use std::{hint::black_box, io::Cursor, time::Instant};
#[cfg(windows)]
fn cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut core::ffi::c_void;
        fn QueryThreadCycleTime(t: *mut core::ffi::c_void, n: *mut u64) -> i32;
    }
    let mut n = 0;
    // SAFETY: the pseudo-handle is valid here and n is writable during the call.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut n) },
        0
    );
    n
}
#[cfg(not(windows))]
fn cycles() -> u64 {
    0
}
fn bench(name: &str, iters: usize, bytes: usize, mut work: impl FnMut()) {
    work();
    for trial in 0..3 {
        let start = Instant::now();
        let cpu = cycles();
        for _ in 0..iters {
            work();
        }
        let cpu = cycles() - cpu;
        let ns = start.elapsed().as_nanos() as f64 / iters as f64;
        println!(
            "{name}\t{trial}\t{iters}\t{ns:.3}\t{:.3}\t{:.3}",
            cpu as f64 / iters as f64,
            bytes as f64 / (1 << 20) as f64 * 1e9 / ns
        );
    }
}
#[path = "support/affinity.rs"]
mod affinity;

fn main() {
    if std::env::args().any(|s| s == "--pin-thread") {
        let affinity = affinity::pin_thread();
        println!("# thread_affinity_mask={affinity:#x}; frequency is uncontrolled");
    }
    let flat_only = std::env::args().any(|s| s == "--flat-only");
    println!(
        "# AdpcmReader state_bytes={}; resident Cursor input; four blocks per data group, 512 groups; optional three held frames per group; caller buffers",
        std::mem::size_of::<AdpcmReader<Cursor<&[u8]>>>()
    );
    println!("operation\ttrial\titers\tns_per_op\tthread_cycles_per_op\tdecoded_MiB_per_s");
    for (name, kind, channels, block_bytes) in [
        ("ms1", AdpcmKind::Microsoft, 1, 256),
        ("ms2", AdpcmKind::Microsoft, 2, 256),
        ("ima4", AdpcmKind::Ima, 2, 256),
        ("ima3", AdpcmKind::Ima3, 2, 248),
    ] {
        let spec = AdpcmSpec {
            kind,
            channels,
            sample_rate: 8000,
            block_bytes,
        };
        let mut fmt = [0; 50];
        let len = spec.format(Endian::Little, &mut fmt).unwrap();
        let codec =
            Adpcm::from_format(Format::parse(&fmt[..len], Endian::Little).unwrap()).unwrap();
        let mut block = vec![0; codec.block_bytes()];
        codec
            .encode_block(&vec![0; codec.scratch_samples()], &mut block)
            .unwrap();
        let child = block.repeat(4);
        let data_frames = codec.frames_per_block() as u64 * 4;
        for segmented in [false, true] {
            if segmented && flat_only {
                continue;
            }
            let bytes = if segmented {
                let mut mux = WavlMuxer::new(
                    Cursor::new(Vec::new()),
                    &fmt[..len],
                    WriteOptions::default(),
                    &[],
                )
                .unwrap();
                for _ in 0..512 {
                    mux.write_data(&child, data_frames).unwrap();
                    mux.write_silence(3).unwrap();
                }
                mux.finish().unwrap().into_inner()
            } else {
                let mut mux = Muxer::new(
                    Cursor::new(Vec::new()),
                    &fmt[..len],
                    WriteOptions::default(),
                    &[],
                )
                .unwrap();
                for _ in 0..512 {
                    mux.write_data(&child, data_frames).unwrap();
                }
                mux.finish().unwrap().into_inner()
            };
            let prefix = format!("{name}/{}", if segmented { "wavl" } else { "flat" });
            let group = data_frames + if segmented { 3 } else { 0 };
            let mut file = AdpcmReader::new(Cursor::new(bytes.as_slice())).unwrap();
            assert_eq!(file.frames(), group * 512);
            let mut out = vec![99; file.frames() as usize * usize::from(channels)];
            assert_eq!(file.read_i16(&mut out).unwrap(), out.len());
            for (frame, values) in out.chunks(usize::from(channels)).enumerate() {
                let at = frame as u64 % group;
                let expected = if kind == AdpcmKind::Ima3 && at < data_frames {
                    (at % codec.frames_per_block() as u64 % 2) as i16
                } else {
                    0
                };
                assert!(values.iter().all(|&n| n == expected));
            }
            bench(&format!("{prefix}/open"), 10000, 0, || {
                black_box(AdpcmReader::new(Cursor::new(black_box(bytes.as_slice()))).unwrap());
            });
            let decoded = out.len() * 2;
            bench(&format!("{prefix}/full_retained"), 16, decoded, || {
                file.seek_frame(0).unwrap();
                black_box(file.read_i16(black_box(&mut out)).unwrap());
            });
            let mut stream = vec![0; 127 * usize::from(channels)];
            bench(&format!("{prefix}/stream127_retained"), 16, decoded, || {
                file.seek_frame(0).unwrap();
                let mut samples = 0;
                loop {
                    let n = file.read_i16(black_box(&mut stream)).unwrap();
                    if n == 0 {
                        break;
                    }
                    samples += n;
                }
                assert_eq!(samples, out.len());
            });
            let mut short = [0; 62];
            let mut next = 0u64;
            bench(&format!("{prefix}/seek_data_62_samples"), 30000, 0, || {
                next = (next + 137) % 512;
                file.seek_frame(next * group + 17).unwrap();
                black_box(file.read_i16(black_box(&mut short)).unwrap());
            });
            if segmented {
                bench(&format!("{prefix}/seek_held"), 30000, 0, || {
                    next = (next + 137) % 512;
                    file.seek_frame(next * group + data_frames + 1).unwrap();
                    black_box(
                        file.read_i16(black_box(&mut short[..usize::from(channels)]))
                            .unwrap(),
                    );
                });
                bench(&format!("{prefix}/same_held_cached"), 300000, 0, || {
                    file.seek_frame(data_frames + 1).unwrap();
                    black_box(
                        file.read_i16(black_box(&mut short[..usize::from(channels)]))
                            .unwrap(),
                    );
                });
            }
        }
    }
}

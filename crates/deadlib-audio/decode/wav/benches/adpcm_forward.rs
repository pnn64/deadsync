//! Resident forward ADPCM open/decode/stream/finish with flat and wavl input.
use deadlib_audio_decode_wav::{
    Adpcm, AdpcmForwardReader, AdpcmKind, AdpcmSpec, Endian, Format, Muxer, WavlMuxer, WriteOptions,
};
use std::{
    hint::black_box,
    io::{self, Cursor, Read},
    time::Instant,
};
struct OnlyRead<'a> {
    inner: Cursor<&'a [u8]>,
}
impl Read for OnlyRead<'_> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.inner.read(b)
    }
}
fn source(b: &[u8]) -> OnlyRead<'_> {
    OnlyRead {
        inner: Cursor::new(b),
    }
}
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
fn main() {
    let flat_only = std::env::args().any(|s| s == "--flat-only");
    println!(
        "# AdpcmForwardReader state_bytes={}; resident Read-only source; 512 groups of four blocks; optional three held frames per group; caller buffers",
        std::mem::size_of::<AdpcmForwardReader<OnlyRead<'_>>>()
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
            let group = data_frames + if segmented { 3 } else { 0 };
            let samples = (group * 512) as usize * usize::from(channels);
            let mut out = vec![99; samples];
            let mut audio = AdpcmForwardReader::new(source(&bytes)).unwrap();
            assert_eq!(audio.frames(), group * 512);
            assert_eq!(audio.read_i16(&mut out).unwrap(), samples);
            audio.finish().unwrap();
            for (frame, values) in out.chunks(usize::from(channels)).enumerate() {
                let at = frame as u64 % group;
                let expected = if kind == AdpcmKind::Ima3 && at < data_frames {
                    (at % codec.frames_per_block() as u64 % 2) as i16
                } else {
                    0
                };
                assert!(values.iter().all(|&n| n == expected));
            }
            let prefix = format!("{name}/{}", if segmented { "wavl" } else { "flat" });
            bench(&format!("{prefix}/open"), 100000, 0, || {
                black_box(AdpcmForwardReader::new(source(black_box(&bytes))).unwrap());
            });
            bench(
                &format!("{prefix}/full_open_read_finish"),
                16,
                samples * 2,
                || {
                    let mut audio = AdpcmForwardReader::new(source(black_box(&bytes))).unwrap();
                    assert_eq!(audio.read_i16(black_box(&mut out)).unwrap(), samples);
                    black_box(audio.finish().unwrap());
                },
            );
            let mut stream = vec![0; 127 * usize::from(channels)];
            bench(
                &format!("{prefix}/stream127_open_finish"),
                16,
                samples * 2,
                || {
                    let mut audio = AdpcmForwardReader::new(source(black_box(&bytes))).unwrap();
                    let mut n = 0;
                    loop {
                        let got = audio.read_i16(black_box(&mut stream)).unwrap();
                        if got == 0 {
                            break;
                        }
                        n += got;
                    }
                    assert_eq!(n, samples);
                    black_box(audio.finish().unwrap());
                },
            );
            bench(&format!("{prefix}/unread_finish"), 1000, 0, || {
                black_box(
                    AdpcmForwardReader::new(source(black_box(&bytes)))
                        .unwrap()
                        .finish()
                        .unwrap(),
                );
            });
        }
    }
}

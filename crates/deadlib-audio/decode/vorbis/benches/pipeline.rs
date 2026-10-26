//! Resident-input timing worker. No PCM retention or file IO in timed regions.
#[path = "support/huffman.rs"]
mod huffman;
#[path = "support/io.rs"]
mod io_support;
#[path = "support/seek.rs"]
mod seek_support;
use deadlib_audio_decode_vorbis::{Imdct, OggEvent, OggOptions, OggReader};
use std::{
    hint::black_box,
    io::{self, Read, Seek, SeekFrom},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef},
    codecs::audio::AudioDecoderOptions,
    formats::{FormatOptions, TrackType, probe::Hint},
    io::{MediaSource, MediaSourceStream},
    meta::MetadataOptions,
};

#[derive(Default)]
struct IoStats {
    calls: AtomicU64,
    bytes: AtomicU64,
}
struct Source {
    data: Arc<[u8]>,
    position: usize,
    cap: usize,
    stats: Arc<IoStats>,
}
impl Read for Source {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let n = output
            .len()
            .min(self.cap)
            .min(self.data.len() - self.position);
        output[..n].copy_from_slice(&self.data[self.position..self.position + n]);
        self.position += n;
        self.stats.calls.fetch_add(1, Ordering::Relaxed);
        self.stats.bytes.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}
impl Seek for Source {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        Err(io::ErrorKind::Unsupported.into())
    }
}
impl MediaSource for Source {
    fn is_seekable(&self) -> bool {
        false
    }
    fn byte_len(&self) -> Option<u64> {
        None
    }
}
#[derive(Debug)]
struct Measurement {
    ns: u128,
    samples: u64,
    reads: u64,
    bytes: u64,
}
fn run(engine: &str, phase: &str, data: &Arc<[u8]>) -> Measurement {
    let stats = Arc::new(IoStats::default());
    let source = Source {
        data: data.clone(),
        position: 0,
        cap: if phase == "stream4096" {
            4096
        } else {
            usize::MAX
        },
        stats: stats.clone(),
    };
    let mut samples = 0;
    let mut first = None;
    let begin = Instant::now();
    if engine == "native" {
        let mut reader = OggReader::with_options(source, OggOptions::default());
        let mut starts = 0;
        while reader
            .read_next(|event| {
                if let OggEvent::StreamStart(_) = &event {
                    starts += 1;
                    assert_eq!(starts, 1, "single-stream benchmark input");
                }
                if let OggEvent::Pcm(pcm) = event {
                    if phase == "startup" && first.is_some() {
                        return;
                    }
                    samples += (pcm.pcm.frames * pcm.pcm.channels) as u64;
                    for plane in pcm.pcm.planes() {
                        black_box(plane);
                    }
                    if pcm.pcm.frames != 0 && first.is_none() {
                        first = Some((
                            begin.elapsed().as_nanos(),
                            stats.calls.load(Ordering::Relaxed),
                            stats.bytes.load(Ordering::Relaxed),
                        ));
                    }
                }
            })
            .unwrap()
        {
            if phase == "startup" && first.is_some() {
                break;
            }
        }
    } else {
        assert_eq!(engine, "symphonia");
        let mut hint = Hint::new();
        hint.with_extension("ogg");
        let mut format = symphonia::default::get_probe()
            .probe(
                &hint,
                MediaSourceStream::new(Box::new(source), Default::default()),
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .unwrap();
        let track = format.default_track(TrackType::Audio).unwrap();
        let track_id = track.id;
        let mut decoder = symphonia::default::get_codecs()
            .make_audio_decoder(
                track.codec_params.as_ref().unwrap().audio().unwrap(),
                &AudioDecoderOptions::default(),
            )
            .unwrap();
        while let Some(packet) = format.next_packet().unwrap() {
            assert_eq!(packet.track_id, track_id, "single-stream benchmark input");
            let GenericAudioBufferRef::F32(audio) = decoder.decode(&packet).unwrap() else {
                panic!("expected f32")
            };
            let channels = audio.spec().channels().count();
            samples += (audio.frames() * channels) as u64;
            for channel in 0..channels {
                black_box(audio.plane(channel).unwrap());
            }
            if audio.frames() != 0 && first.is_none() {
                first = Some((
                    begin.elapsed().as_nanos(),
                    stats.calls.load(Ordering::Relaxed),
                    stats.bytes.load(Ordering::Relaxed),
                ));
            }
            if phase == "startup" && first.is_some() {
                break;
            }
        }
    }
    let full_ns = begin.elapsed().as_nanos();
    let (ns, reads, bytes) = if phase == "startup" {
        first.expect("benchmark requires a nonempty stream")
    } else {
        (
            full_ns,
            stats.calls.load(Ordering::Relaxed),
            stats.bytes.load(Ordering::Relaxed),
        )
    };
    Measurement {
        ns,
        samples,
        reads,
        bytes,
    }
}
fn main() {
    // cargo bench's automatic --bench argument carries no worker semantics.
    let args: Vec<_> = std::env::args()
        .skip(1)
        .filter(|a| a != "--bench")
        .collect();
    assert!(
        args.len() == 4 || args.len() == 5,
        "ENGINE PHASE ITERATIONS FILE_OR_BLOCK_SIZE [SEEK_REQUESTS]"
    );
    let engine = &args[0];
    let phase = &args[1];
    let iterations: usize = args[2].parse().unwrap();
    assert!(iterations > 0);
    if phase == "seek_open" || phase == "seek_batch" {
        assert_eq!(
            args.len(),
            5,
            "seek timing requires a reference request file"
        );
        let workload =
            seek_support::Workload::read(std::path::Path::new(&args[4]), phase == "seek_batch");
        let data: Arc<[u8]> = std::fs::read(&args[3]).unwrap().into();
        let stats = Arc::new(io_support::IoStats::default());
        let open = || {
            seek_support::Session::open(
                engine,
                io_support::Source::new(&data, usize::MAX, true, &stats),
                workload.geometry,
            )
        };
        let mut elapsed = 0;
        let mut exposed = 0;
        let units;
        if phase == "seek_batch" {
            let mut session = open();
            session.warm(workload.geometry);
            session.batch(&workload, true);
            stats.reset();
            let begin = Instant::now();
            for _ in 0..iterations {
                exposed += session.batch(&workload, false);
            }
            elapsed = begin.elapsed().as_nanos();
            units = workload.expected_samples() * iterations as u64;
        } else {
            drop(open());
            stats.reset();
            for _ in 0..iterations {
                let begin = Instant::now();
                let session = open();
                black_box(&session);
                drop(session);
                elapsed += begin.elapsed().as_nanos();
            }
            units = iterations as u64;
        }
        let [reads, bytes, seeks, tells] = stats.values();
        println!(
            "{engine}\t{phase}\t{iterations}\t{elapsed}\t{units}\t{reads}\t{bytes}\t{seeks}\t{tells}\t{exposed}"
        );
        return;
    }
    assert_eq!(args.len(), 4);
    if phase == "huffman" {
        assert_eq!(engine, "native");
        huffman::measure(&std::fs::read(&args[3]).unwrap(), iterations);
        return;
    }
    if phase == "imdct" {
        assert_eq!(engine, "native");
        let n: usize = args[3].parse().unwrap();
        let mut transform = Imdct::new(n).unwrap();
        let spectrum: Vec<_> = (0..n / 2)
            .map(|i| ((i * 17 % 101) as i32 - 50) as f32 / 256.0)
            .collect();
        let mut output = vec![0.0; n];
        transform.inverse(&spectrum, &mut output).unwrap();
        let begin = Instant::now();
        for _ in 0..iterations {
            transform
                .inverse(black_box(&spectrum), black_box(&mut output))
                .unwrap();
            black_box(&output);
        }
        println!(
            "native\timdct\t{iterations}\t{}\t{}\t0\t0",
            begin.elapsed().as_nanos(),
            iterations * n
        );
        return;
    }
    assert!(["full", "stream4096", "startup"].contains(&phase.as_str()));
    let data: Arc<[u8]> = std::fs::read(&args[3]).unwrap().into();
    // Initialize lazy registries and warm caches before recording stream work.
    black_box(run(engine, phase, &data));
    let mut total = Measurement {
        ns: 0,
        samples: 0,
        reads: 0,
        bytes: 0,
    };
    for _ in 0..iterations {
        let measured = run(engine, phase, &data);
        total.ns += measured.ns;
        total.samples += measured.samples;
        total.reads += measured.reads;
        total.bytes += measured.bytes;
    }
    println!(
        "{engine}\t{phase}\t{iterations}\t{}\t{}\t{}\t{}",
        total.ns, total.samples, total.reads, total.bytes
    );
}

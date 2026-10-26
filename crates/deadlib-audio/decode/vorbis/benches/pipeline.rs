//! Resident-input timing worker. No PCM retention or file IO in timed regions.
use deadlib_audio_decode_vorbis::{OggEvent, OggOptions, OggReader};
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
    assert_eq!(
        args.len(),
        4,
        "ENGINE PHASE ITERATIONS FILE (native|symphonia; full|stream4096|startup)"
    );
    let engine = &args[0];
    let phase = &args[1];
    assert!(["full", "stream4096", "startup"].contains(&phase.as_str()));
    let iterations: usize = args[2].parse().unwrap();
    assert!(iterations > 0);
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

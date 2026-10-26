use crate::io_support::Source;
use deadlib_audio_decode_vorbis::{OggEvent, OggSeekReader};
use std::{fs::File, hint::black_box, io::Read, path::Path};
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef},
    codecs::audio::{AudioDecoder, AudioDecoderOptions},
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
    units::Timestamp,
};

#[derive(Clone, Copy)]
pub struct Geometry {
    pub channels: usize,
    pub rate: u32,
    pub large_block: u32,
    pub floor0: bool,
    pub frames: u64,
}
pub struct Request {
    pub target: u64,
    pub want: usize,
    pub planes: Vec<Vec<f32>>,
}
pub struct Workload {
    pub geometry: Geometry,
    pub requests: Vec<Request>,
}
impl Workload {
    pub fn read(path: &Path, with_pcm: bool) -> Self {
        let mut file = File::open(path).unwrap();
        let mut header = [0; 36];
        file.read_exact(&mut header).unwrap();
        assert_eq!(&header[..8], b"DLVSEEK1");
        let u32_at = |n| u32::from_le_bytes(header[n..n + 4].try_into().unwrap());
        let geometry = Geometry {
            channels: u32_at(8) as usize,
            rate: u32_at(12),
            large_block: u32_at(16),
            floor0: u32_at(20) != 0,
            frames: u64::from_le_bytes(header[24..32].try_into().unwrap()),
        };
        assert!(
            (1..=255).contains(&geometry.channels) && geometry.rate != 0 && geometry.frames != 0
        );
        assert!(
            (64..=8192).contains(&geometry.large_block) && geometry.large_block.is_power_of_two()
        );
        let count = u32_at(32);
        assert!((1..=10000).contains(&count));
        let mut requests = Vec::new();
        if with_pcm {
            for _ in 0..count {
                let mut record = [0; 12];
                file.read_exact(&mut record).unwrap();
                let target = u64::from_le_bytes(record[..8].try_into().unwrap());
                let want = u32::from_le_bytes(record[8..].try_into().unwrap()) as usize;
                assert!(want > 0 && want <= 4096 && target + want as u64 <= geometry.frames);
                let mut planes = Vec::new();
                for _ in 0..geometry.channels {
                    let mut plane = Vec::new();
                    for _ in 0..want {
                        let mut bytes = [0; 4];
                        file.read_exact(&mut bytes).unwrap();
                        let value = f32::from_le_bytes(bytes);
                        assert!(value.is_finite());
                        plane.push(value);
                    }
                    planes.push(plane);
                }
                requests.push(Request {
                    target,
                    want,
                    planes,
                });
            }
            let mut extra = [0];
            assert_eq!(file.read(&mut extra).unwrap(), 0);
        }
        Self { geometry, requests }
    }
    pub fn expected_samples(&self) -> u64 {
        self.requests
            .iter()
            .map(|r| r.want as u64 * self.geometry.channels as u64)
            .sum()
    }
}

// Keep the native reader on the stack: boxing it would introduce a harness
// allocation into the codec construction measurements.
#[allow(clippy::large_enum_variant)]
pub enum Session {
    Native(OggSeekReader<Source>),
    Symphonia {
        format: Box<dyn FormatReader>,
        decoder: Box<dyn AudioDecoder>,
        track_id: u32,
    },
}
impl Session {
    pub fn open(engine: &str, source: Source, geometry: Geometry) -> Self {
        if engine == "native" {
            let reader = OggSeekReader::new(source).unwrap();
            assert_eq!(reader.index().container().streams().len(), 1);
            let stream = reader.selected();
            assert_eq!(stream.end.frames, geometry.frames);
            assert_eq!(
                stream.end.start_granule, 0,
                "seek benchmark requires zero timestamp origin"
            );
            assert_eq!(
                stream.info.identification.channels as usize,
                geometry.channels
            );
            assert_eq!(stream.info.identification.sample_rate, geometry.rate);
            Self::Native(reader)
        } else {
            assert_eq!(engine, "symphonia");
            let mut hint = Hint::new();
            hint.with_extension("ogg");
            let format = symphonia::default::get_probe()
                .probe(
                    &hint,
                    MediaSourceStream::new(Box::new(source), Default::default()),
                    FormatOptions::default(),
                    MetadataOptions::default(),
                )
                .unwrap();
            let track = format.default_track(TrackType::Audio).unwrap();
            // Symphonia's track range can include the negative-PTS priming
            // packet (e.g. -128 for the stereo fixture). Compare retained PCM
            // counts after warm decode and positions by PTS + trim_start.
            let track_id = track.id;
            let parameters = track.codec_params.as_ref().unwrap().audio().unwrap();
            assert_eq!(parameters.sample_rate, Some(geometry.rate));
            let decoder = symphonia::default::get_codecs()
                .make_audio_decoder(parameters, &AudioDecoderOptions::default())
                .unwrap();
            Self::Symphonia {
                format,
                decoder,
                track_id,
            }
        }
    }
    pub fn warm(&mut self, geometry: Geometry) {
        let mut samples = 0;
        match self {
            Self::Native(reader) => {
                reader.seek(reader.selected().info.id, 0).unwrap();
                while reader
                    .read_next(|event| {
                        if let OggEvent::Pcm(pcm) = event {
                            samples += (pcm.pcm.frames * pcm.pcm.channels) as u64;
                            for plane in pcm.pcm.planes() {
                                black_box(plane);
                            }
                        }
                    })
                    .unwrap()
                {}
            }
            Self::Symphonia {
                format,
                decoder,
                track_id,
            } => {
                format
                    .seek(
                        SeekMode::Accurate,
                        SeekTo::Timestamp {
                            ts: Timestamp::new(0),
                            track_id: *track_id,
                        },
                    )
                    .unwrap();
                decoder.reset();
                while let Some(packet) = format.next_packet().unwrap() {
                    assert_eq!(packet.track_id, *track_id);
                    let GenericAudioBufferRef::F32(audio) = decoder.decode(&packet).unwrap() else {
                        panic!("expected f32")
                    };
                    samples += (audio.frames() * geometry.channels) as u64;
                    for channel in 0..geometry.channels {
                        black_box(audio.plane(channel).unwrap());
                    }
                }
            }
        }
        assert_eq!(
            samples,
            geometry.frames * geometry.channels as u64,
            "warm full-decode count"
        );
    }
    /// Returns samples exposed by the API, which can exceed the requested
    /// samples (notably the native page-at-a-time API). This is not a count of
    /// hidden internally synthesized frames in libvorbisfile.
    pub fn batch(&mut self, workload: &Workload, validate: bool) -> u64 {
        let mut exposed = 0;
        let geometry = workload.geometry;
        for request in &workload.requests {
            let mut position = 0;
            let mut difference = Difference::default();
            match self {
                Self::Native(reader) => {
                    reader
                        .seek(reader.selected().info.id, request.target)
                        .unwrap();
                    assert_eq!(reader.position(), request.target);
                    while position < request.want {
                        let more = reader
                            .read_next(|event| {
                                if let OggEvent::Pcm(pcm) = event {
                                    exposed += (pcm.pcm.frames * pcm.pcm.channels) as u64;
                                    if position == request.want {
                                        return;
                                    }
                                    assert_eq!(pcm.start_frame, request.target + position as u64);
                                    let count = pcm.pcm.frames.min(request.want - position);
                                    for (channel, plane) in pcm.pcm.planes().enumerate() {
                                        let actual = &plane[..count];
                                        black_box(actual);
                                        if validate {
                                            difference.add(
                                                actual,
                                                &request.planes[channel]
                                                    [position..position + count],
                                            );
                                        }
                                    }
                                    position += count;
                                }
                            })
                            .unwrap();
                        assert!(
                            more || position == request.want,
                            "premature native seek EOF"
                        );
                    }
                }
                Self::Symphonia {
                    format,
                    decoder,
                    track_id,
                } => {
                    let pre = request.target.saturating_sub(geometry.large_block as u64);
                    let timestamp = Timestamp::new(pre as i64);
                    let seeked = format
                        .seek(
                            SeekMode::Accurate,
                            SeekTo::Timestamp {
                                ts: timestamp,
                                track_id: *track_id,
                            },
                        )
                        .unwrap();
                    assert_eq!(seeked.required_ts, timestamp);
                    assert!(seeked.actual_ts <= timestamp);
                    decoder.reset();
                    while position < request.want {
                        let packet = format
                            .next_packet()
                            .unwrap()
                            .expect("premature Symphonia seek EOF");
                        assert_eq!(packet.track_id, *track_id);
                        let GenericAudioBufferRef::F32(audio) = decoder.decode(&packet).unwrap()
                        else {
                            panic!("expected f32")
                        };
                        exposed += (audio.frames() * geometry.channels) as u64;
                        if audio.frames() == 0 {
                            continue;
                        }
                        let start = packet.pts.get() + packet.trim_start.get() as i64;
                        let next = request.target as i64 + position as i64;
                        assert!(start <= next, "seek missed required pre-roll");
                        let skip = (next - start).min(audio.frames() as i64) as usize;
                        let count = (audio.frames() - skip).min(request.want - position);
                        for channel in 0..geometry.channels {
                            let actual = &audio.plane(channel).unwrap()[skip..skip + count];
                            black_box(actual);
                            if validate {
                                difference.add(
                                    actual,
                                    &request.planes[channel][position..position + count],
                                );
                            }
                        }
                        position += count;
                    }
                }
            }
            assert_eq!(position, request.want);
            if validate {
                difference.check(geometry.floor0);
            }
        }
        exposed
    }
}
#[derive(Default)]
struct Difference {
    peak: f64,
    squared: f64,
    signal: f64,
    count: u64,
}
impl Difference {
    fn add(&mut self, actual: &[f32], expected: &[f32]) {
        assert_eq!(actual.len(), expected.len());
        for (&actual, &expected) in actual.iter().zip(expected) {
            assert!(actual.is_finite());
            let error = (actual as f64 - expected as f64).abs();
            self.peak = self.peak.max(error);
            self.squared += error * error;
            self.signal = self.signal.max((expected as f64).abs());
            self.count += 1;
        }
    }
    fn check(&self, floor0: bool) {
        assert!(self.count > 0);
        let (peak, rms) = if floor0 { (5e-5, 3e-6) } else { (3e-6, 5e-7) };
        let scale = self.signal.max(1.0);
        assert!(
            self.peak <= peak * scale && (self.squared / self.count as f64).sqrt() <= rms * scale,
            "seek PCM mismatch: peak {}, RMS {}",
            self.peak,
            (self.squared / self.count as f64).sqrt()
        );
    }
}

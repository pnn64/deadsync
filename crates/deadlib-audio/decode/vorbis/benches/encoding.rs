//! Test-only resident-PCM encoding and independent round-trip measurement.
use deadlib_audio_decode_ogg::PacketWriter;
use deadlib_audio_decode_vorbis::{
    Channel, Comments, EncodeOptions, OggEvent, OggOptions, OggSeekReader, PacketEncoder,
    channel_order, decode_reader,
};
use std::{
    fs,
    hint::black_box,
    io::{self, Cursor, Write},
    path::Path,
    time::Instant,
};
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef, Position},
    codecs::audio::AudioDecoderOptions,
    formats::{FormatOptions, TrackType, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

struct Pcm {
    channels: usize,
    rate: u32,
    frames: usize,
    samples: Vec<f32>,
}
impl Pcm {
    fn load(path: &Path) -> Self {
        let bytes = fs::read(path).unwrap();
        assert_eq!(&bytes[..8], b"DLVPCM1\0");
        let channels = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let rate = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let frames =
            usize::try_from(u64::from_le_bytes(bytes[16..24].try_into().unwrap())).unwrap();
        assert!((1..=255).contains(&channels) && rate > 0);
        assert_eq!(
            bytes.len(),
            24 + frames
                .checked_mul(channels)
                .unwrap()
                .checked_mul(4)
                .unwrap()
        );
        let samples = bytes[24..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        Self {
            channels,
            rate,
            frames,
            samples,
        }
    }
    fn save(&self, path: &Path) {
        let mut bytes = b"DLVPCM1\0".to_vec();
        bytes.extend_from_slice(&(self.channels as u32).to_le_bytes());
        bytes.extend_from_slice(&self.rate.to_le_bytes());
        bytes.extend_from_slice(&(self.frames as u64).to_le_bytes());
        for value in &self.samples {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        fs::write(path, bytes).unwrap();
    }
}

fn prepare(source: &Path, destination: &Path, seconds: u64, skip_seconds: u64) {
    let mut reader = OggSeekReader::new(Cursor::new(fs::read(source).unwrap())).unwrap();
    assert_eq!(
        reader.index().streams().len(),
        1,
        "single-stream benchmark input"
    );
    let stream = reader.selected();
    let channels = stream.info.identification.channels as usize;
    let rate = stream.info.identification.sample_rate;
    let limit = (seconds * rate as u64).min(stream.end.frames);
    let start = if stream.end.frames > limit.saturating_mul(3) {
        (skip_seconds * rate as u64).min(stream.end.frames - limit)
    } else {
        0
    };
    let id = stream.info.id;
    reader.seek(id, start).unwrap();
    let mut samples = Vec::with_capacity(limit as usize * channels);
    let mut frames = 0;
    while frames < limit
        && reader
            .read_packet(|event| {
                if let OggEvent::Pcm(block) = event {
                    assert_eq!(block.start_frame, start + frames);
                    let count = (block.pcm.frames as u64).min(limit - frames) as usize;
                    for frame in 0..count {
                        for channel in 0..channels {
                            samples.push(block.pcm.plane(channel).unwrap()[frame]);
                        }
                    }
                    frames += count as u64;
                }
            })
            .unwrap()
    {}
    assert_eq!(frames, limit);
    assert!(frames > 0 && samples.iter().all(|x| x.is_finite() && x.abs() <= 4.0));
    Pcm {
        channels,
        rate,
        frames: frames as usize,
        samples,
    }
    .save(destination);
    println!(
        "{{\"channels\":{channels},\"rate\":{rate},\"frames\":{frames},\"start_frame\":{start}}}"
    );
}

struct Sink {
    bytes: u64,
    output: Option<Vec<u8>>,
}
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        black_box(bytes); // Preserve codec/CRC work even when output is discarded.
        self.bytes += bytes.len() as u64;
        if let Some(output) = &mut self.output {
            output.extend_from_slice(bytes);
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[derive(Default)]
struct Measured {
    full_ns: u128,
    open_ns: u128,
    first_ns: u128,
    bytes: u64,
    packets: u64,
    first_input: usize,
    first_granule: u64,
    first_packet_bytes: usize,
}
fn run(pcm: &Pcm, passes: u8, retain: bool) -> (Measured, Option<Vec<u8>>) {
    let begin = Instant::now();
    let mut encoder = PacketEncoder::new(EncodeOptions {
        channels: pcm.channels as u8,
        sample_rate: pcm.rate,
        residue_passes: passes,
        ..EncodeOptions::default()
    })
    .unwrap();
    let id = encoder.identification().encode().unwrap();
    let comments = Comments {
        vendor: b"deadlib encoding benchmark",
        comments: vec![b"TITLE=encoding benchmark"],
    }
    .encode(&Default::default())
    .unwrap();
    let mut pages = PacketWriter::new(
        Sink {
            bytes: 0,
            output: retain.then(Vec::new),
        },
        123,
    );
    pages.write_packet(&id, 0, false).unwrap();
    pages.write_packet(&comments, 0, false).unwrap();
    pages
        .write_packet(encoder.setup_packet(), 0, false)
        .unwrap();
    pages.flush_page().unwrap();
    let mut result = Measured {
        open_ns: begin.elapsed().as_nanos(),
        ..Measured::default()
    };
    let mut supplied = 0;
    let mut emit = |packet: deadlib_audio_decode_vorbis::EncodedPacket<'_>, supplied: usize| {
        pages.write_packet(packet.data, packet.granule_position, packet.last_in_stream)?;
        result.packets += 1;
        if result.first_ns == 0 && packet.granule_position > 0 {
            result.first_ns = begin.elapsed().as_nanos();
            result.first_input = supplied;
            result.first_granule = packet.granule_position;
            result.first_packet_bytes = packet.data.len();
        }
        Ok(())
    };
    for chunk in pcm.samples.chunks(4096 * pcm.channels) {
        supplied += chunk.len() / pcm.channels;
        encoder
            .push_interleaved(chunk, |packet| emit(packet, supplied))
            .unwrap();
    }
    encoder.finish(|packet| emit(packet, supplied)).unwrap();
    assert_eq!(encoder.position(), pcm.frames as u64);
    let sink = pages.into_inner();
    result.bytes = sink.bytes;
    drop(encoder);
    result.full_ns = begin.elapsed().as_nanos();
    assert!(result.first_ns > 0);
    (result, sink.output)
}
fn encode(pcm: &Pcm, passes: u8, iterations: usize, output: &str) {
    assert!(iterations > 0 && (output == "-" || iterations == 1));
    let (warm, _) = run(pcm, passes, false);
    let mut total = Measured::default();
    for _ in 0..iterations {
        let (value, bytes) = run(pcm, passes, output != "-");
        assert_eq!(
            (
                value.bytes,
                value.packets,
                value.first_input,
                value.first_granule,
                value.first_packet_bytes
            ),
            (
                warm.bytes,
                warm.packets,
                warm.first_input,
                warm.first_granule,
                warm.first_packet_bytes
            )
        );
        total.full_ns += value.full_ns;
        total.open_ns += value.open_ns;
        total.first_ns += value.first_ns;
        if let Some(bytes) = bytes {
            fs::write(output, bytes).unwrap();
        }
    }
    println!(
        "native\t{passes}\t{iterations}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        total.full_ns,
        total.open_ns,
        total.first_ns,
        pcm.frames,
        warm.bytes,
        warm.packets,
        warm.first_input,
        warm.first_granule,
        warm.first_packet_bytes
    );
}

fn native_pcm(bytes: &[u8], input: &Pcm) -> Vec<Vec<f32>> {
    let mut output = vec![Vec::new(); input.channels];
    let mut starts = 0;
    let mut ends = 0;
    decode_reader(bytes, OggOptions::default(), |event| match event {
        OggEvent::StreamStart(info) => {
            starts += 1;
            assert_eq!(info.identification.channels as usize, input.channels);
            assert_eq!(info.identification.sample_rate, input.rate);
        }
        OggEvent::Pcm(block) => {
            assert_eq!(block.start_frame as usize, output[0].len());
            for (out, plane) in output.iter_mut().zip(block.pcm.planes()) {
                out.extend_from_slice(plane);
            }
        }
        OggEvent::StreamEnd(end) => {
            ends += 1;
            assert_eq!(end.frames as usize, input.frames);
        }
        _ => panic!("unexpected encoded-stream event"),
    })
    .unwrap();
    assert_eq!((starts, ends), (1, 1));
    assert!(output.iter().all(|p| p.len() == input.frames));
    output
}
fn symphonia_pcm(bytes: &[u8], input: &Pcm) -> Vec<Vec<f32>> {
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default()),
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap();
    let track = format.default_track(TrackType::Audio).unwrap();
    let id = track.id;
    assert_eq!(
        track
            .codec_params
            .as_ref()
            .unwrap()
            .audio()
            .unwrap()
            .sample_rate,
        Some(input.rate)
    );
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(
            track.codec_params.as_ref().unwrap().audio().unwrap(),
            &AudioDecoderOptions::default(),
        )
        .unwrap();
    let mut output = vec![Vec::new(); input.channels];
    while let Some(packet) = format.next_packet().unwrap() {
        assert_eq!(packet.track_id, id);
        let GenericAudioBufferRef::F32(audio) = decoder.decode(&packet).unwrap() else {
            panic!("expected f32");
        };
        assert_eq!(audio.spec().channels().count(), input.channels);
        assert_eq!(audio.spec().rate(), input.rate);
        for (ch, out) in output.iter_mut().enumerate() {
            let position = match channel_order(input.channels as u8).unwrap()[ch] {
                Channel::Mono | Channel::FrontLeft => Position::FRONT_LEFT,
                Channel::FrontCenter => Position::FRONT_CENTER,
                Channel::FrontRight => Position::FRONT_RIGHT,
                Channel::SideLeft => Position::SIDE_LEFT,
                Channel::SideRight => Position::SIDE_RIGHT,
                Channel::RearLeft => Position::REAR_LEFT,
                Channel::RearCenter => Position::REAR_CENTER,
                Channel::RearRight => Position::REAR_RIGHT,
                Channel::Lfe => Position::LFE1,
            };
            let index = audio
                .spec()
                .channels()
                .get_canonical_index_for_positioned_channel(position)
                .unwrap();
            out.extend_from_slice(audio.plane(index).unwrap());
        }
    }
    assert!(output.iter().all(|p| p.len() == input.frames));
    output
}
#[derive(Default)]
struct Difference {
    peak: f64,
    energy: f64,
    reference_peak: f64,
    samples: u64,
}
impl Difference {
    fn add(&mut self, a: f32, b: f32) {
        assert!(a.is_finite() && b.is_finite());
        let delta = a as f64 - b as f64;
        self.peak = self.peak.max(delta.abs());
        self.energy += delta * delta;
        self.reference_peak = self.reference_peak.max(b.abs() as f64);
        self.samples += 1;
    }
    fn rms(&self) -> f64 {
        (self.energy / self.samples.max(1) as f64).sqrt()
    }
    fn check(&self) {
        assert!(
            self.peak <= 3e-6 * self.reference_peak.max(1.0),
            "decoder peak {}",
            self.peak
        );
        assert!(
            self.rms() <= 5e-7 * self.reference_peak.max(1.0),
            "decoder RMS {}",
            self.rms()
        );
    }
}
fn verify(input: &Pcm, path: &Path, reference: &Pcm) {
    assert_eq!(
        (input.channels, input.rate, input.frames),
        (reference.channels, reference.rate, reference.frames)
    );
    let bytes = fs::read(path).unwrap();
    let output = native_pcm(&bytes, input);
    let symphonia = symphonia_pcm(&bytes, input);
    let mut c = Difference::default();
    let mut sym = Difference::default();
    let mut source = Difference::default();
    let mut signal = 0.0;
    for frame in 0..input.frames {
        for ch in 0..input.channels {
            let i = frame * input.channels + ch;
            let value = output[ch][frame];
            c.add(value, reference.samples[i]);
            sym.add(value, symphonia[ch][frame]);
            source.add(value, input.samples[i]);
            signal += (input.samples[i] as f64).powi(2);
        }
    }
    c.check();
    sym.check();
    if signal == 0.0 {
        assert_eq!(source.peak, 0.0, "silence must remain exact zero");
    }
    let snr = 10.0 * (signal / source.energy).log10();
    let snr = if snr.is_finite() {
        snr.to_string()
    } else {
        "null".to_owned()
    };
    println!(
        "{{\"frames\":{},\"channels\":{},\"rate\":{},\"source_snr_db\":{snr},\"source_peak_error\":{},\"source_rms_error\":{},\"libvorbis_peak_error\":{},\"libvorbis_rms_error\":{},\"symphonia_peak_error\":{},\"symphonia_rms_error\":{}}}",
        input.frames,
        input.channels,
        input.rate,
        source.peak,
        source.rms(),
        c.peak,
        c.rms(),
        sym.peak,
        sym.rms()
    );
}
fn compare(input: &Pcm, reference: &Pcm) {
    assert_eq!(
        (input.channels, input.rate, input.frames),
        (reference.channels, reference.rate, reference.frames)
    );
    let mut difference = Difference::default();
    for (&a, &b) in input.samples.iter().zip(&reference.samples) {
        difference.add(a, b);
    }
    difference.check();
    println!(
        "{{\"samples\":{},\"peak_abs_error\":{},\"rms_error\":{}}}",
        difference.samples,
        difference.peak,
        difference.rms()
    );
}
fn exact(input: &Pcm, before: &Path, after: &Path) {
    let before = fs::read(before).unwrap();
    let after = fs::read(after).unwrap();
    for decode in [native_pcm, symphonia_pcm] {
        let a = decode(&before, input);
        let b = decode(&after, input);
        for (a, b) in a.iter().zip(&b) {
            assert!(
                a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits()),
                "encoded PCM changed"
            );
        }
    }
    println!(
        "{{\"samples_per_decoder\":{},\"native_bit_exact\":true,\"symphonia_bit_exact\":true}}",
        input.frames * input.channels
    );
}
fn main() {
    let args: Vec<_> = std::env::args()
        .skip(1)
        .filter(|a| a != "--bench")
        .collect();
    match args.first().map(String::as_str) {
        Some("prepare") => {
            assert_eq!(args.len(), 5);
            prepare(
                Path::new(&args[1]),
                Path::new(&args[2]),
                args[3].parse().unwrap(),
                args[4].parse().unwrap(),
            );
        }
        Some("encode") => {
            assert_eq!(args.len(), 5);
            encode(
                &Pcm::load(Path::new(&args[3])),
                args[1].parse().unwrap(),
                args[2].parse().unwrap(),
                &args[4],
            );
        }
        Some("verify") => {
            assert_eq!(args.len(), 4);
            verify(
                &Pcm::load(Path::new(&args[1])),
                Path::new(&args[2]),
                &Pcm::load(Path::new(&args[3])),
            );
        }
        Some("compare") => {
            assert_eq!(args.len(), 3);
            compare(
                &Pcm::load(Path::new(&args[1])),
                &Pcm::load(Path::new(&args[2])),
            );
        }
        Some("exact") => {
            assert_eq!(args.len(), 4);
            exact(
                &Pcm::load(Path::new(&args[1])),
                Path::new(&args[2]),
                Path::new(&args[3]),
            );
        }
        _ => panic!(
            "prepare OGG PCM SECONDS SKIP_SECONDS | encode STAGES ITERATIONS PCM OUTPUT_OR_DASH | verify SOURCE_PCM OGG C_REFERENCE_PCM | compare PCM PCM"
        ),
    }
}

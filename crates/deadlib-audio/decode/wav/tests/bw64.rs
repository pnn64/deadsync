use deadlib_audio_decode_wav::{
    AdpcmData, AdpcmForwardReader, AdpcmKind, AdpcmReader, AdpcmSpec, AdpcmWriter, Container,
    DataLength, ForwardReader, Muxer, Reader, SampleFormat, Spec, Wave, WavlWriter, WriteOptions,
    Writer,
};
use std::cell::Cell;
use std::io::{self, Cursor, Write};
use std::rc::Rc;

fn options() -> WriteOptions {
    WriteOptions {
        container: Container::Bw64,
        auto_rf64: false,
        force_extensible: false,
    }
}
fn ck(id: &[u8; 4], bytes: &[u8]) -> Vec<u8> {
    let mut result = id.to_vec();
    result.extend((bytes.len() as u32).to_le_bytes());
    result.extend(bytes);
    if bytes.len() & 1 != 0 {
        result.push(0);
    }
    result
}
fn form(id: &[u8; 4], dummy: u64, fact: Option<u32>, trailing: bool, wavl: bool) -> Vec<u8> {
    // Independently packed header-only IMA block: exactly one sample, 12345.
    let fmt = [
        17, 0, 1, 0, 0x40, 0x1f, 0, 0, 0, 0x7d, 0, 0, 4, 0, 4, 0, 2, 0, 1, 0,
    ];
    let audio = ck(b"data", &[0x39, 0x30, 0, 0]);
    let mut body = ck(b"fmt ", &fmt);
    let fact = fact
        .map(|n| ck(b"fact", &n.to_le_bytes()))
        .unwrap_or_default();
    if !trailing {
        body.extend(&fact);
    }
    body.extend(if wavl {
        ck(b"LIST", &[b"wavl".as_slice(), &audio].concat())
    } else {
        audio
    });
    if trailing {
        body.extend(fact);
    }
    let mut ds = Vec::new();
    ds.extend((40 + body.len() as u64).to_le_bytes());
    ds.extend(4u64.to_le_bytes());
    ds.extend(dummy.to_le_bytes());
    ds.extend(0u32.to_le_bytes());
    [id.as_slice(), &[255; 4], b"WAVE", &ck(b"ds64", &ds), &body].concat()
}

#[test]
fn bw64_dummy_words_are_retained_raw_but_never_determine_adpcm_duration() {
    for dummy in [0, 1, u64::MAX] {
        for fact in [None, Some(1), Some(u32::MAX)] {
            for trailing in [false, true] {
                let bytes = form(b"BW64", dummy, fact, trailing, false);
                let wave = Wave::parse(&bytes).unwrap();
                assert_eq!(wave.ds64.unwrap().sample_count, dummy);
                assert_eq!(wave.fact, fact.map(u64::from));
                let raw = Reader::new(Cursor::new(&bytes)).unwrap();
                assert_eq!(raw.ds64_sample_count(), Some(dummy));
                assert_eq!(raw.fact(), fact.map(u64::from));
                if fact == Some(1) {
                    let mut out = [0];
                    AdpcmData::new(&wave)
                        .unwrap()
                        .read_frames(0, &mut out, &mut [0])
                        .unwrap();
                    assert_eq!(out, [12345]);
                    let mut file = AdpcmReader::from_reader(raw).unwrap();
                    assert_eq!(file.read_i16(&mut out).unwrap(), 1);
                    assert_eq!(out, [12345]);
                    let raw = ForwardReader::new(bytes.as_slice()).unwrap();
                    let mut forward = AdpcmForwardReader::from_reader_with_frames(raw, 1).unwrap();
                    assert_eq!(forward.read_i16(&mut out).unwrap(), 1);
                    assert_eq!(out, [12345]);
                    forward.finish().unwrap();
                } else {
                    assert!(AdpcmData::new(&wave).is_err());
                    assert!(AdpcmReader::from_reader(raw).is_err());
                    let raw = ForwardReader::new(bytes.as_slice()).unwrap();
                    assert!(
                        AdpcmForwardReader::from_reader_with_frames(raw, 1)
                            .and_then(AdpcmForwardReader::finish)
                            .is_err()
                    );
                }
                let inferred = AdpcmForwardReader::new(bytes.as_slice());
                assert_eq!(inferred.is_ok(), fact == Some(1) && !trailing);
            }
        }
    }
    // Identical bytes retain the RF64 compatibility fallback and sentinel.
    for fact in [None, Some(1), Some(u32::MAX)] {
        let bytes = form(b"RF64", 1, fact, false, false);
        assert_eq!(
            AdpcmData::new(&Wave::parse(&bytes).unwrap())
                .unwrap()
                .frames(),
            1
        );
        AdpcmForwardReader::new(bytes.as_slice())
            .unwrap()
            .finish()
            .unwrap();
    }
}

fn zero_dummy(bytes: &[u8]) {
    assert_eq!(&bytes[..4], b"BW64");
    assert_eq!(&bytes[12..16], b"ds64");
    assert_eq!(&bytes[36..44], &[0; 8]);
}

#[test]
fn every_bw64_writer_zeros_dummy_words_in_both_finalization_paths() {
    for sample in [
        SampleFormat::U8,
        SampleFormat::I16,
        SampleFormat::I24,
        SampleFormat::I32,
        SampleFormat::F32,
        SampleFormat::F64,
    ] {
        let spec = Spec::new(sample, 2, 48000);
        let raw = (0..usize::from(sample.bits() / 8) * 2)
            .map(|n| (n * 73 + 129) as u8)
            .collect::<Vec<_>>();
        let mut known = Writer::with_frames(Vec::new(), spec, options(), &[], 1).unwrap();
        known.write_frames(&raw).unwrap();
        let bytes = known.finish_stream().unwrap();
        zero_dummy(&bytes);
        assert_eq!(Wave::parse(&bytes).unwrap().data().unwrap(), &raw);
        let mut seek = Writer::new(Cursor::new(Vec::new()), spec, options(), &[]).unwrap();
        seek.write_frames(&raw).unwrap();
        assert_eq!(seek.finish().unwrap().into_inner(), bytes);
        let length = DataLength {
            bytes: 24 + raw.len() as u64 + (raw.len() & 1) as u64,
            frames: 4,
        };
        let mut known = WavlWriter::with_lengths(Vec::new(), spec, options(), &[], length).unwrap();
        known.write_frames(&raw).unwrap();
        known.write_silence(3).unwrap();
        let bytes = known.finish_stream().unwrap();
        zero_dummy(&bytes);
        assert_eq!(Wave::parse(&bytes).unwrap().pcm().unwrap().frames(), 4);
        let mut seek = WavlWriter::new(Cursor::new(Vec::new()), spec, options(), &[]).unwrap();
        seek.write_frames(&raw).unwrap();
        seek.write_silence(3).unwrap();
        assert_eq!(seek.finish().unwrap().into_inner(), bytes);
    }
    for kind in [AdpcmKind::Microsoft, AdpcmKind::Ima, AdpcmKind::Ima3] {
        let spec = AdpcmSpec {
            kind,
            channels: 1,
            sample_rate: 8000,
            block_bytes: if kind == AdpcmKind::Ima3 { 40 } else { 32 },
        };
        let mut known = AdpcmWriter::with_frames(Vec::new(), spec, options(), &[], 1).unwrap();
        known.write_i16(&[12345]).unwrap();
        let bytes = known.finish_stream().unwrap();
        zero_dummy(&bytes);
        let mut seek = AdpcmWriter::new(Cursor::new(Vec::new()), spec, options(), &[]).unwrap();
        seek.write_i16(&[12345]).unwrap();
        assert_eq!(seek.finish().unwrap().into_inner(), bytes);
    }
}

#[derive(Clone, Default)]
struct Sink(Rc<Cell<usize>>);
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.set(self.0.get() + bytes.len());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn bw64_fact_limits_fail_before_io_and_pcm_keeps_u64_frame_counts() {
    let count = u64::from(u32::MAX) + 1;
    let opaque = [
        0x34, 0x12, 1, 0, 0x40, 0x1f, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0,
    ];
    let sink = Sink::default();
    assert!(
        Muxer::with_lengths(
            sink.clone(),
            &opaque,
            options(),
            &[],
            DataLength {
                bytes: 0,
                frames: count
            }
        )
        .is_err()
    );
    assert!(
        WavlWriter::with_lengths(
            sink.clone(),
            Spec::new(SampleFormat::U8, 1, 8000),
            options(),
            &[],
            DataLength {
                bytes: 4,
                frames: count
            }
        )
        .is_err()
    );
    assert!(
        Writer::with_frames(
            sink.clone(),
            Spec::new(SampleFormat::F32, 1, 8000),
            options(),
            &[],
            count
        )
        .is_err()
    );
    let adpcm = AdpcmSpec {
        kind: AdpcmKind::Ima,
        channels: 1,
        sample_rate: 8000,
        block_bytes: 4,
    };
    assert!(AdpcmWriter::with_frames(sink.clone(), adpcm, options(), &[], count).is_err());
    assert_eq!(sink.0.get(), 0);
    let writer = Writer::with_frames(
        Vec::new(),
        Spec::new(SampleFormat::U8, 1, 8000),
        options(),
        &[],
        count,
    )
    .unwrap();
    let header = writer.into_inner();
    zero_dummy(&header);
    assert_eq!(
        ForwardReader::new(header.as_slice())
            .unwrap()
            .frames()
            .unwrap(),
        count
    );
    let mut mux = Muxer::new(Cursor::new(Vec::new()), &opaque, options(), &[]).unwrap();
    mux.write_data(&[], u64::from(u32::MAX)).unwrap();
    assert!(mux.write_data(&[77], 1).is_err());
    assert_eq!(mux.bytes_written(), 0);
    assert_eq!(mux.frames_written(), u64::from(u32::MAX));
    let bytes = mux.finish().unwrap().into_inner();
    zero_dummy(&bytes);
    assert!(Wave::parse(&bytes).unwrap().data().unwrap().is_empty());
    let mut wavl = WavlWriter::new(
        Cursor::new(Vec::new()),
        Spec::new(SampleFormat::U8, 1, 8000),
        options(),
        &[],
    )
    .unwrap();
    wavl.write_silence(u64::from(u32::MAX)).unwrap();
    assert!(wavl.write_silence(1).is_err());
    assert_eq!(wavl.frames_written(), u64::from(u32::MAX));
    let bytes = wavl.finish().unwrap().into_inner();
    zero_dummy(&bytes);
    let wave = Wave::parse(&bytes).unwrap();
    assert_eq!(wave.pcm().unwrap().frames(), u64::from(u32::MAX));
    ForwardReader::new(bytes.as_slice())
        .unwrap()
        .finish()
        .unwrap();
}

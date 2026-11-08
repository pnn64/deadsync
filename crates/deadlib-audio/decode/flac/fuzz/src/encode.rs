use deadlib_audio_decode_flac::*;
use std::io::Cursor;
pub fn encode_input(data: &[u8]) -> Option<(EncodeOptions, Vec<i32>)> {
    if data.len() < 6 || data.len() > 8192 {
        return None;
    }
    let bits = 4 + data[0] % 29;
    let channels = 1 + data[1] % 8;
    let options = EncodeOptions {
        bits_per_sample: bits,
        channels,
        compression_level: data[2] % 9,
        block_size: [16, 32, 64, 128, 256, 4096][data[3] as usize % 6],
        variable_block_size: data[4] & 1 != 0,
        ..Default::default()
    };
    let mut pcm: Vec<_> = data[6..]
        .as_chunks::<4>()
        .0
        .iter()
        .take(1024 * channels as usize)
        .map(|p| i32::from_le_bytes(*p) >> (32 - bits))
        .collect();
    pcm.truncate(pcm.len() / channels as usize * channels as usize);
    Some((options, pcm))
}
pub fn encode(data: &[u8]) {
    let Some((options, pcm)) = encode_input(data) else {
        return;
    };
    let (bits, channels) = (options.bits_per_sample, options.channels);
    let pcm = pcm.as_slice();
    let mut native = Encoder::new(Cursor::new(Vec::new()), options).unwrap();
    native.write_interleaved(pcm).unwrap();
    let whole = native.finish_seekable().unwrap().output.into_inner();
    let mut native = Encoder::new(Cursor::new(Vec::new()), options).unwrap();
    for chunk in pcm.chunks((data[5] as usize + 1) * channels as usize) {
        let planes: Vec<Vec<i32>> = (0..channels as usize)
            .map(|ch| {
                chunk
                    .iter()
                    .skip(ch)
                    .step_by(channels as usize)
                    .copied()
                    .collect()
            })
            .collect();
        native
            .write_planar(&planes.iter().map(Vec::as_slice).collect::<Vec<_>>())
            .unwrap();
    }
    assert_eq!(whole, native.finish_seekable().unwrap().output.into_inner());
    let mut decoder = Decoder::new(&whole).unwrap();
    let mut out = Vec::new();
    while let Some(frame) = decoder.read_next().unwrap() {
        for i in 0..frame.pcm.samples() {
            for ch in 0..channels as usize {
                out.push(frame.pcm.plane(ch)[i]);
            }
        }
    }
    assert_eq!(out, pcm);
    assert_eq!(decoder.integrity(), Integrity::Verified);
    let mut encoder = OggEncoder::new(Vec::new(), options, OggWriteOptions::default()).unwrap();
    encoder.write_interleaved(pcm).unwrap();
    let ogg = encoder.finish().unwrap().output;
    let mut reader = OggReader::new(ogg.as_slice(), OggOptions::default());
    out.clear();
    while reader
        .read_next(|e| {
            if let OggEvent::Pcm(frame) = e {
                for i in 0..frame.pcm.samples() {
                    for ch in 0..channels as usize {
                        out.push(frame.pcm.plane(ch)[i]);
                    }
                }
            }
        })
        .unwrap()
    {}
    assert_eq!(out, pcm);
    // The pinned independent decoder has documented 32-bit stereo defects.
    // That configuration still runs every native exact check above; its two
    // named reference failures remain separate regression gates.
    if bits != 32 || channels != 2 {
        symphonia(&whole, pcm, bits, channels);
    }
}
fn symphonia(data: &[u8], pcm: &[i32], bits: u8, channels: u8) {
    use symphonia::core::{
        audio::{Audio, GenericAudioBufferRef},
        codecs::audio::{AudioCodecParameters, AudioDecoderOptions, well_known::CODEC_ID_FLAC},
        packet::PacketRef,
        units::{Duration, Timestamp},
    };
    let meta = Metadata::parse(data, Limits::default()).unwrap();
    let mut params = AudioCodecParameters::new();
    params.for_codec(CODEC_ID_FLAC).with_extra_data(
        meta.stream_info
            .encode()
            .unwrap()
            .to_vec()
            .into_boxed_slice(),
    );
    let mut reference = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .unwrap();
    let mut decoder = FrameDecoder::new(Some(meta.stream_info)).unwrap();
    let mut offset = meta.audio_offset;
    let mut sample = 0;
    while offset < data.len() {
        let frame = decoder.decode(&data[offset..]).unwrap();
        let packet = PacketRef::new(
            0,
            Timestamp::ZERO,
            Duration::ZERO,
            &data[offset..offset + frame.bytes],
        );
        let GenericAudioBufferRef::S32(audio) = reference.decode_ref(&packet).unwrap() else {
            panic!("PCM type");
        };
        assert_eq!(audio.frames(), frame.pcm.samples());
        for ch in 0..channels as usize {
            for (i, &value) in audio.plane(ch).unwrap().iter().enumerate() {
                assert_eq!(
                    value >> (32 - bits),
                    pcm[(sample + i) * channels as usize + ch]
                );
            }
        }
        sample += frame.pcm.samples();
        offset += frame.bytes;
    }
    assert_eq!(sample * channels as usize, pcm.len());
}

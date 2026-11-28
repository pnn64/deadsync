use crate::{
    DEFAULT_FPS, Info, MAX_FPS, YuvConversion, avi, matroska, mp4,
    packet::{Codec, Packet, Track},
};
use deadlib_image::RgbaImage;
use deadlib_video_h264::{
    ChromaFormat, SurfaceDecoder, SurfaceFrame,
    nal::{NalUnitType, SpsInfo, annex_b_units, avcc_units, parse_avcc_config, parse_sps_info},
};
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
    sync::Arc,
};

const MAX_CODED_PIXELS: u64 = 4096 * 2304;

pub(crate) struct Video {
    source: Source,
    pub(crate) info: Info,
}

enum Source {
    Indexed {
        input: BufReader<File>,
        track: Track,
        reorder_depth: usize,
    },
    Mpeg {
        bytes: Vec<u8>,
        frame_rate: f64,
    },
}

impl Video {
    pub(crate) fn open(path: &Path, looped: bool) -> Result<Option<Self>, String> {
        let mut input = BufReader::new(File::open(path).map_err(|error| error.to_string())?);
        let track = match mp4::index(&mut input).map_err(|error| error.to_string())? {
            Some(track) => track,
            None => match avi::index(&mut input).map_err(|error| error.to_string())? {
                Some(track) => track,
                None => match matroska::index(&mut input).map_err(|error| error.to_string())? {
                    Some(track) => track,
                    None => return open_mpeg(input, looped),
                },
            },
        };
        let avc = match &track.codec {
            Codec::Avc(bytes) => {
                let config = parse_avcc_config(bytes)?;
                let sps = config
                    .sps_nals
                    .first()
                    .ok_or("missing AVC sequence parameter set")?;
                Some(parse_sps_info(sps).map_err(|error| error.to_string())?)
            }
            Codec::AnnexBAvc(config) => {
                let mut sps = annex_b_sps(config)?;
                if sps.is_none() {
                    stream_packets(
                        &mut input,
                        &track.packets[..track.packets.len().min(16)],
                        || false,
                        |packet| {
                            sps = annex_b_sps(packet)?;
                            Ok(sps.is_none())
                        },
                    )?;
                }
                let Some(sps) = sps else { return Ok(None) };
                Some(sps)
            }
            Codec::Hevc(bytes) => {
                let header = bytes.get(..23).ok_or("truncated HEVC configuration")?;
                if header[16] & 3 != 1 || header[17] & 7 != 0 || header[18] & 7 != 0 {
                    return Ok(None);
                }
                None
            }
            _ => None,
        };
        let (width, height, matrix, full_range, reorder_depth) = if let Some(sps) = avc {
            if !supported(&sps) {
                return Ok(None);
            }
            (
                sps.width,
                sps.height,
                u16::from(sps.matrix),
                sps.full_range,
                sps.max_reorder_frames.unwrap_or(16).min(16) as usize,
            )
        } else {
            (track.width, track.height, 2, false, 0)
        };
        if !valid_size(width, height) {
            return Ok(None);
        }
        let matrix = track.color.map_or(matrix, |color| color.matrix);
        let full_range = track
            .color
            .and_then(|color| color.full_range)
            .unwrap_or(full_range);
        let coeffs = match matrix {
            1 | 7 => [1.5748, -0.187_324, -0.468_124, 1.8556],
            9 | 10 => [1.4746, -0.164_553, -0.571_353, 1.8814],
            _ => [1.402, -0.344_136, -0.714_136, 1.772],
        };
        let conversion = if full_range {
            YuvConversion::full(coeffs)
        } else {
            YuvConversion::limited(coeffs)
        };
        let timescale = f64::from(track.timescale);
        let duration_sec =
            (track.reported_duration.unwrap_or(track.duration) as f64 / timescale) as f32;
        let fps = if track.decode_duration == 0 {
            DEFAULT_FPS
        } else {
            (track.packets.len() as f64 * timescale / track.decode_duration as f64) as f32
        }
        .clamp(1.0, MAX_FPS);
        Ok(Some(Self {
            source: Source::Indexed {
                input,
                track,
                reorder_depth,
            },
            info: Info {
                width,
                height,
                fps,
                duration_sec: Some(duration_sec),
                looped,
                conversion,
            },
        }))
    }

    pub(crate) fn poster(&mut self) -> Result<RgbaImage, String> {
        let info = self.info;
        let mut poster = None;
        self.decode_one_pass(
            || false,
            |frame, _| {
                poster = Some(to_rgba(frame, info)?);
                Ok(false)
            },
        )?;
        poster.ok_or_else(|| "video has no displayed picture".into())
    }

    /// Emits display frames and their number of output ticks at the app's CFR.
    /// A false return stops decoding without draining the queue.
    pub(crate) fn decode_one_pass(
        &mut self,
        cancelled: impl FnMut() -> bool,
        mut emit: impl FnMut(&Picture, u64) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let info = self.info;
        let (input, track, reorder_depth) = match &mut self.source {
            Source::Indexed {
                input,
                track,
                reorder_depth,
            } => (input, track, *reorder_depth),
            Source::Mpeg { bytes, frame_rate } => {
                return decode_mpeg(bytes, f64::from(info.fps) / *frame_rate, cancelled, emit);
            }
        };
        let pts = &track.presentation_times;
        let duration = track.duration;
        let ticks_per_time = f64::from(info.fps) / f64::from(track.timescale);
        let to_tick = |time: i64| (time as f64 * ticks_per_time).round() as i64;
        if let Codec::Mpeg4 { config, avi: true } = &track.codec
            && is_packed_mpeg4(input, track, config)?
        {
            return decode_packed_mpeg4(input, track, config, info, cancelled, emit);
        }
        let first_tick = to_tick(pts[0]).max(0);
        let last_tick = (duration as f64 * ticks_per_time).round() as i64;
        let mut index = 0;
        let consume = |frame: Picture| {
            let time = pts.get(index).copied();
            let start = time.map(to_tick).unwrap_or(last_tick).max(first_tick);
            index += 1;
            let end = pts
                .get(index)
                .copied()
                .map(to_tick)
                .unwrap_or(last_tick)
                .min(last_tick);
            if time.is_some_and(|time| time >= 0 && (time as u64) < duration) {
                emit(&frame, end.saturating_sub(start).max(0) as u64)
            } else {
                Ok(true)
            }
        };
        let packets = &track.packets;
        match &track.codec {
            Codec::Avc(_) | Codec::AnnexBAvc(_) => {
                decode_avc(input, track, info, reorder_depth, cancelled, consume)
            }
            Codec::Hevc(config) => {
                let mut decoder =
                    deadlib_video_hevc::Decoder::with_limits(deadlib_video_hevc::DecodeLimits {
                        max_picture_pixels: MAX_CODED_PIXELS,
                        ..Default::default()
                    })
                    .with_display_order();
                let length = decoder
                    .push_decoder_config(config)
                    .map_err(|error| error.to_string())?;
                let mut consume = consume;
                if !stream_packets(input, packets, cancelled, |packet| {
                    let mut result = Ok(true);
                    decoder
                        .push_packet_pictures(packet, length, |picture| {
                            if matches!(result, Ok(true)) {
                                result = consume(Picture::Hevc(picture));
                            }
                        })
                        .map_err(|error| error.to_string())?;
                    result
                })? {
                    return Ok(false);
                }
                while let Some(picture) =
                    decoder.flush_picture().map_err(|error| error.to_string())?
                {
                    if !consume(Picture::Hevc(picture))? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Codec::Vp9 => {
                let mut decoder =
                    deadlib_video_vp9::Decoder::with_limits(deadlib_video_vp9::DecoderLimits {
                        max_luma_samples: MAX_CODED_PIXELS,
                        max_width: info.width,
                        max_height: info.height,
                    });
                let mut consume = consume;
                stream_packets(input, packets, cancelled, |packet| {
                    match decoder
                        .push_picture(packet)
                        .map_err(|error| error.to_string())?
                    {
                        Some(picture) => consume(Picture::Vp9(picture)),
                        None => Ok(true),
                    }
                })
            }
            Codec::Mpeg1 => {
                let mut decoder = deadlib_video_mpeg2::Decoder::with_mpeg1_recovery();
                let mut consume = consume;
                let mut output = |frame, result: &mut Result<bool, String>| {
                    if matches!(result, Ok(true)) {
                        *result = consume(Picture::Mpeg(frame));
                    }
                };
                if !stream_packets(input, packets, cancelled, |packet| {
                    let mut result = Ok(true);
                    for unit in deadlib_video_mpeg2::units(packet) {
                        decoder
                            .push(unit, |frame| output(frame, &mut result))
                            .map_err(|error| error.to_string())?;
                        if !matches!(result, Ok(true)) {
                            break;
                        }
                    }
                    result
                })? {
                    return Ok(false);
                }
                let mut result = Ok(true);
                decoder
                    .finish(|frame| output(frame, &mut result))
                    .map_err(|error| error.to_string())?;
                result
            }
            Codec::Mpeg4 { config, .. } => {
                let mut consume = consume;
                decode_mpeg4(input, packets, config, cancelled, |frame, _| {
                    consume(Picture::Mpeg4(frame))
                })
            }
            Codec::MsMpeg4v2 => {
                let mut decoder = deadlib_video_mpeg4::MsMpeg4v2Decoder::new(
                    info.width as usize,
                    info.height as usize,
                )
                .map_err(|error| error.to_string())?;
                let mut consume = consume;
                stream_packets(input, packets, cancelled, |packet| {
                    if packet.is_empty() {
                        return Ok(true);
                    }
                    let frame = decoder
                        .push_packet(packet)
                        .map_err(|error| error.to_string())?;
                    consume(Picture::Mpeg4(frame))
                })
            }
        }
    }
}

fn decode_mpeg4(
    input: &mut BufReader<File>,
    packets: &[Packet],
    config: &[u8],
    cancelled: impl FnMut() -> bool,
    mut output: impl FnMut(Arc<deadlib_video_mpeg4::Frame>, Option<usize>) -> Result<bool, String>,
) -> Result<bool, String> {
    let mut decoder = deadlib_video_mpeg4::Decoder::new();
    let mut index = 0usize;
    let mut decode = |packet: &[u8]| {
        for unit in deadlib_video_mpeg4::units(packet) {
            if let Some(frame) = decoder.push(unit).map_err(|error| error.to_string())?
                && !output(frame, Some(index.saturating_sub(1)))?
            {
                return Ok(false);
            }
        }
        index += 1;
        Ok(true)
    };
    if !decode(config)? || !stream_packets(input, packets, cancelled, decode)? {
        return Ok(false);
    }
    if let Some(frame) = decoder.finish() {
        return output(frame, None);
    }
    Ok(true)
}

fn is_packed_mpeg4(
    input: &mut BufReader<File>,
    track: &Track,
    config: &[u8],
) -> Result<bool, String> {
    fn marker(bytes: &[u8]) -> bool {
        deadlib_video_mpeg4::units(bytes).any(|unit| {
            if unit.code != 0xb2 || !unit.payload.starts_with(b"DivX") {
                return false;
            }
            unit.payload
                .iter()
                .rfind(|&&byte| byte != 0 && byte != b' ')
                == Some(&b'p')
        })
    }
    if marker(config) {
        return Ok(true);
    }
    let mut packed = false;
    stream_packets(
        input,
        &track.packets[..1],
        || false,
        |packet| {
            packed = marker(packet);
            Ok(false)
        },
    )?;
    Ok(packed)
}

fn decode_packed_mpeg4(
    input: &mut BufReader<File>,
    track: &Track,
    config: &[u8],
    info: Info,
    cancelled: impl FnMut() -> bool,
    mut emit: impl FnMut(&Picture, u64) -> Result<bool, String>,
) -> Result<bool, String> {
    let container_tick = |sample: i64| {
        (sample as f64 * f64::from(info.fps) / f64::from(track.timescale)).round() as u64
    };
    let end = container_tick(track.duration as i64).saturating_add(1);
    let mut first = None;
    let mut pending: Option<(Arc<deadlib_video_mpeg4::Frame>, u64)> = None;
    let complete = decode_mpeg4(input, &track.packets, config, cancelled, |frame, packet| {
        let (time, scale) = frame
            .presentation_time()
            .ok_or("missing MPEG-4 picture time")?;
        let was_empty = first.is_none();
        let (origin, decode_delay) = *first.get_or_insert_with(|| {
            let packet = packet.unwrap_or(track.packets.len() - 1);
            (time, container_tick(track.presentation_times[packet]))
        });
        // Packed AVI can reconstruct several pictures from one chunk. The
        // VOP times preserve gaps after dummy chunks. The first picture
        // also covers the leading ticks before the decoder starts output.
        let tick = if was_empty {
            0
        } else {
            let elapsed = time
                .checked_sub(origin)
                .ok_or("MPEG-4 picture timestamp overflow")?;
            let tick = (elapsed as f64 * f64::from(info.fps) / scale as f64).round() as u64;
            let limit = if packet.is_none() {
                end.saturating_sub(1)
            } else {
                end
            };
            tick.saturating_add(decode_delay).min(limit)
        };
        if let Some((picture, from)) = pending.replace((frame, tick)) {
            return emit(&Picture::Mpeg4(picture), tick.saturating_sub(from));
        }
        Ok(true)
    })?;
    if complete && let Some((frame, from)) = pending {
        return emit(&Picture::Mpeg4(frame), end.saturating_sub(from));
    }
    Ok(complete)
}

fn annex_b_sps(bytes: &[u8]) -> Result<Option<SpsInfo>, String> {
    annex_b_units(bytes)
        .find(|nal| nal.nal_unit_type == NalUnitType::Sps)
        .map(|nal| parse_sps_info(&nal).map_err(|error| error.to_string()))
        .transpose()
}

fn decode_avc(
    input: &mut BufReader<File>,
    track: &Track,
    info: Info,
    reorder_depth: usize,
    cancelled: impl FnMut() -> bool,
    mut consume: impl FnMut(Picture) -> Result<bool, String>,
) -> Result<bool, String> {
    let mut decoder = SurfaceDecoder::with_max_depth(reorder_depth);
    let length_size = match &track.codec {
        Codec::Avc(config) => {
            let config = parse_avcc_config(config)?;
            for nal in config.sps_nals.iter().chain(&config.pps_nals) {
                check_parameter_set(nal, info)?;
                decoder
                    .decode_nal(nal, |_| {})
                    .map_err(|error| error.to_string())?;
            }
            Some(config.length_size)
        }
        Codec::AnnexBAvc(config) => {
            for nal in annex_b_units(config) {
                check_parameter_set(&nal, info)?;
                decoder
                    .decode_nal(&nal, |_| {})
                    .map_err(|error| error.to_string())?;
            }
            None
        }
        _ => unreachable!(),
    };
    let mut output = |frame, result: &mut Result<bool, String>| {
        if matches!(result, Ok(true)) {
            *result = consume(Picture::Avc(frame));
        }
    };
    if !stream_packets(input, &track.packets, cancelled, |packet| {
        let annex_b = if length_size.is_none() { packet } else { &[] };
        let units = avcc_units(packet, length_size.unwrap_or(0)).chain(annex_b_units(annex_b));
        let mut result = Ok(true);
        for nal in units {
            if nal.is_inter_idr() {
                // A few legacy clips put a corrupt inter slice in the first
                // IDR access unit. Wait for the following usable picture.
                continue;
            }
            check_parameter_set(&nal, info)?;
            decoder
                .decode_nal(&nal, |frame| output(frame, &mut result))
                .map_err(|error| error.to_string())?;
            if !matches!(result, Ok(true)) {
                break;
            }
        }
        result
    })? {
        return Ok(false);
    }
    let mut result = Ok(true);
    decoder.flush(|frame| output(frame, &mut result));
    result
}

fn open_mpeg(mut input: BufReader<File>, looped: bool) -> Result<Option<Video>, String> {
    const MAX_COMPRESSED_BYTES: u64 = 256 << 20;
    input.rewind().map_err(|error| error.to_string())?;
    let mut header = [0; 4];
    if input.read_exact(&mut header).is_err()
        || !matches!(&header, b"\x00\x00\x01\xb3" | b"\x00\x00\x01\xba")
    {
        return Ok(None);
    }
    let file_size = input
        .get_ref()
        .metadata()
        .map_err(|error| error.to_string())?
        .len();
    if file_size > MAX_COMPRESSED_BYTES {
        return Ok(None);
    }
    input.rewind().map_err(|error| error.to_string())?;
    let mut bytes = Vec::with_capacity(file_size as usize);
    input
        .take(MAX_COMPRESSED_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_COMPRESSED_BYTES {
        return Ok(None);
    }
    let program_stream = header[3] == 0xba;
    if program_stream {
        bytes = deadlib_video_mpeg2::video_elementary_stream(&bytes)
            .map_err(|error| error.to_string())?;
    }
    let video = deadlib_video_mpeg2::sequence_info(&bytes).map_err(|error| error.to_string())?;
    if !valid_size(video.width, video.height) {
        return Ok(None);
    }
    let frame_rate = f64::from(video.frame_rate.0) / f64::from(video.frame_rate.1);
    let frames = deadlib_video_mpeg2::units(&bytes)
        .filter(|unit| unit.code == 0x00)
        .count();
    if frames == 0 {
        return Ok(None);
    }
    // Raw streams keep the legacy 25 output ticks per second. Count pictures at
    // the sequence rate to get the duration, since the bitrate header may
    // declare a maximum rate for variable-bit-rate video.
    let fps = if program_stream { frame_rate } else { 25.0 };
    let duration = frames as f64 / frame_rate;
    Ok(Some(Video {
        source: Source::Mpeg { bytes, frame_rate },
        info: Info {
            width: video.width,
            height: video.height,
            fps: (fps as f32).clamp(1.0, MAX_FPS),
            duration_sec: Some(duration as f32),
            looped,
            conversion: YuvConversion::limited([1.402, -0.344_136, -0.714_136, 1.772]),
        },
    }))
}

fn decode_mpeg(
    bytes: &[u8],
    ratio: f64,
    mut cancelled: impl FnMut() -> bool,
    mut emit: impl FnMut(&Picture, u64) -> Result<bool, String>,
) -> Result<bool, String> {
    let mut decoder = deadlib_video_mpeg2::Decoder::with_mpeg1_recovery();
    let mut tick = 0;
    let mut index = 0u64;
    let mut output = |frame, result: &mut Result<bool, String>| {
        if matches!(result, Ok(true)) {
            index += 1;
            let end = (index as f64 * ratio).round() as u64;
            *result = emit(&Picture::Mpeg(frame), end - tick);
            tick = end;
        }
    };
    for unit in deadlib_video_mpeg2::units(bytes) {
        if cancelled() {
            return Ok(false);
        }
        let mut result = Ok(true);
        decoder
            .push(unit, |frame| output(frame, &mut result))
            .map_err(|error| error.to_string())?;
        if !matches!(result, Ok(true)) {
            return result;
        }
    }
    let mut result = Ok(true);
    decoder
        .finish(|frame| output(frame, &mut result))
        .map_err(|error| error.to_string())?;
    result
}

fn stream_packets(
    input: &mut BufReader<File>,
    packets: &[Packet],
    mut cancelled: impl FnMut() -> bool,
    mut consume: impl FnMut(&[u8]) -> Result<bool, String>,
) -> Result<bool, String> {
    let mut packet = Vec::new();
    let mut position = None;
    for sample in packets {
        if cancelled() {
            return Ok(false);
        }
        if position != Some(sample.offset) {
            input
                .seek(SeekFrom::Start(sample.offset))
                .map_err(|error| error.to_string())?;
        }
        packet.resize(sample.size as usize, 0);
        input
            .read_exact(&mut packet)
            .map_err(|error| error.to_string())?;
        position = Some(sample.offset + u64::from(sample.size));
        if !consume(&packet)? {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) enum Picture {
    Avc(SurfaceFrame),
    Hevc(deadlib_video_hevc::Picture),
    Vp9(deadlib_video_vp9::Picture),
    Mpeg(Arc<deadlib_video_mpeg2::Frame>),
    Mpeg4(Arc<deadlib_video_mpeg4::Frame>),
}

fn valid_size(width: u32, height: u32) -> bool {
    width != 0
        && height != 0
        && width.is_multiple_of(2)
        && height.is_multiple_of(2)
        && u64::from(width) * u64::from(height) <= MAX_CODED_PIXELS
}

fn supported(sps: &SpsInfo) -> bool {
    sps.width != 0
        && sps.height != 0
        && u64::from(sps.coded_width) * u64::from(sps.coded_height) <= MAX_CODED_PIXELS
        && sps.width.is_multiple_of(2)
        && sps.height.is_multiple_of(2)
        && matches!(sps.chroma_format, 1 | 2)
        && matches!(sps.bit_depth_luma, 8 | 10)
        && sps.bit_depth_chroma == sps.bit_depth_luma
        && sps.progressive
}

fn check_parameter_set(
    nal: &deadlib_video_h264::nal::NalUnit<'_>,
    info: Info,
) -> Result<(), String> {
    if nal.nal_unit_type == NalUnitType::Sps {
        let sps = parse_sps_info(nal).map_err(|error| error.to_string())?;
        if !supported(&sps) || (sps.width, sps.height) != (info.width, info.height) {
            return Err("AVC parameter set changed dimensions or sample format".into());
        }
    }
    Ok(())
}

fn check_picture(frame: &Picture, info: Info) -> Result<(), String> {
    let (size, eight_bit_420) = match frame {
        Picture::Avc(frame) => (frame.size(), matches!(frame.bit_depth(), 8 | 10)),
        Picture::Hevc(frame) => (
            (frame.width(), frame.height()),
            frame.bit_depth() == 8 && frame.chroma_format_idc() == 1,
        ),
        Picture::Vp9(frame) => (
            (frame.width(), frame.height()),
            frame.bit_depth() == 8 && frame.subsampling() == [1, 1],
        ),
        Picture::Mpeg(frame) => ((frame.width() as u32, frame.height() as u32), true),
        Picture::Mpeg4(frame) => ((frame.width() as u32, frame.height() as u32), true),
    };
    if size != (info.width, info.height) || !eight_bit_420 {
        return Err("video picture changed dimensions or sample format".into());
    }
    Ok(())
}

pub(crate) fn pack(frame: &Picture, info: Info, bytes: &mut [u8]) -> Result<(), String> {
    check_picture(frame, info)?;
    let mut at = 0;
    match frame {
        Picture::Avc(frame) => {
            if frame.bit_depth() == 8 && frame.chroma_format() == ChromaFormat::Yuv420 {
                for index in 0..3 {
                    let plane = frame.plane(index).ok_or("missing AVC plane")?;
                    pack_rows(bytes, &mut at, plane.rows())?;
                }
            } else {
                pack_avc_420(frame, info, bytes)?;
                at = bytes.len();
            }
        }
        Picture::Hevc(frame) => {
            for plane in frame.planes() {
                pack_rows(bytes, &mut at, plane.rows_u8().ok_or("missing HEVC plane")?)?;
            }
        }
        Picture::Vp9(frame) => {
            for plane in frame.planes() {
                pack_rows(bytes, &mut at, plane.rows_u8().ok_or("missing VP9 plane")?)?;
            }
        }
        Picture::Mpeg(frame) => {
            for plane in frame.planes() {
                pack_rows(bytes, &mut at, plane.rows())?;
            }
        }
        Picture::Mpeg4(frame) => {
            for plane in frame.planes() {
                pack_rows(bytes, &mut at, plane.rows())?;
            }
        }
    }
    if at != bytes.len() {
        return Err("incomplete video upload buffer".into());
    }
    Ok(())
}

fn pack_rows<'a>(
    bytes: &mut [u8],
    at: &mut usize,
    rows: impl Iterator<Item = &'a [u8]>,
) -> Result<(), String> {
    for row in rows {
        let end = *at + row.len();
        bytes
            .get_mut(*at..end)
            .ok_or("video plane exceeds the upload buffer")?
            .copy_from_slice(row);
        *at = end;
    }
    Ok(())
}

fn pack_avc_420(frame: &SurfaceFrame, info: Info, bytes: &mut [u8]) -> Result<(), String> {
    let ten_bit = frame.bit_depth() == 10;
    let vertical_subsample = frame.chroma_format() == ChromaFormat::Yuv422;
    let mut cursor = 0;
    for index in 0..3 {
        let plane = frame.plane(index).ok_or("missing AVC plane")?;
        let width = (info.width as usize) >> usize::from(index != 0);
        let mut rows = plane.rows();
        while let Some(row) = rows.next() {
            let next = if index != 0 && vertical_subsample {
                Some(rows.next().ok_or("missing AVC chroma row")?)
            } else {
                None
            };
            let out = bytes
                .get_mut(cursor..cursor + width)
                .ok_or("AVC plane exceeds the upload buffer")?;
            let sample = |row: &[u8], x| {
                if ten_bit {
                    u16::from_le_bytes([row[2 * x], row[2 * x + 1]])
                } else {
                    u16::from(row[x])
                }
            };
            let shift = 2 * u32::from(ten_bit) + u32::from(next.is_some());
            let rounding = (1 << shift) >> 1;
            for (x, dst) in out.iter_mut().enumerate() {
                let sum = sample(row, x) + next.map_or(0, |next| sample(next, x));
                *dst = ((sum + rounding) >> shift).min(255) as u8;
            }
            cursor += width;
        }
    }
    if cursor != bytes.len() {
        return Err("incomplete AVC upload buffer".into());
    }
    Ok(())
}

pub(crate) fn to_rgba(frame: &Picture, info: Info) -> Result<RgbaImage, String> {
    check_picture(frame, info)?;
    match frame {
        Picture::Avc(frame) => {
            if frame.bit_depth() != 8 || frame.chroma_format() != ChromaFormat::Yuv420 {
                let mut pixels = vec![0; crate::yuv420_frame_bytes(info)];
                pack_avc_420(frame, info, &mut pixels)?;
                let width = info.width as usize;
                let (y, chroma) = pixels.split_at(width * info.height as usize);
                let (u, v) = chroma.split_at(chroma.len() / 2);
                return rgba_rows(
                    info,
                    y.chunks_exact(width),
                    u.chunks_exact(width / 2),
                    v.chunks_exact(width / 2),
                );
            }
            let y = frame.plane(0).ok_or("missing AVC luma plane")?;
            let u = frame.plane(1).ok_or("missing AVC U plane")?;
            let v = frame.plane(2).ok_or("missing AVC V plane")?;
            rgba_rows(info, y.rows(), u.rows(), v.rows())
        }
        Picture::Hevc(frame) => {
            let [y, u, v] = frame.planes();
            rgba_rows(
                info,
                y.rows_u8().ok_or("missing HEVC luma plane")?,
                u.rows_u8().ok_or("missing HEVC U plane")?,
                v.rows_u8().ok_or("missing HEVC V plane")?,
            )
        }
        Picture::Vp9(frame) => {
            let [y, u, v] = frame.planes();
            rgba_rows(
                info,
                y.rows_u8().ok_or("missing VP9 luma plane")?,
                u.rows_u8().ok_or("missing VP9 U plane")?,
                v.rows_u8().ok_or("missing VP9 V plane")?,
            )
        }
        Picture::Mpeg(frame) => {
            let [y, u, v] = frame.planes();
            rgba_rows(info, y.rows(), u.rows(), v.rows())
        }
        Picture::Mpeg4(frame) => {
            let [y, u, v] = frame.planes();
            rgba_rows(info, y.rows(), u.rows(), v.rows())
        }
    }
}

fn rgba_rows<'a>(
    info: Info,
    mut y: impl Iterator<Item = &'a [u8]>,
    u: impl Iterator<Item = &'a [u8]>,
    v: impl Iterator<Item = &'a [u8]>,
) -> Result<RgbaImage, String> {
    let mut image = RgbaImage::new(info.width, info.height);
    let [y_scale, y_offset, c_scale, c_offset] = info.conversion.levels;
    let [r_cr, g_cb, g_cr, b_cb] = info.conversion.coeffs;
    let encode = |value: f32| (value * 255.0).round() as u8;
    for (out, (u, v)) in image
        .as_mut()
        .chunks_exact_mut(info.width as usize * 8)
        .zip(u.zip(v))
    {
        let first = y.next().ok_or("missing video luma row")?;
        let second = y.next().ok_or("missing video luma row")?;
        let (top, bottom) = out.split_at_mut(info.width as usize * 4);
        for (x, (&u, &v)) in u.iter().zip(v).enumerate() {
            let cb = f32::from(u) * (c_scale / 255.0) + c_offset;
            let cr = f32::from(v) * (c_scale / 255.0) + c_offset;
            let (r, g, b) = (r_cr * cr, g_cb * cb + g_cr * cr, b_cb * cb);
            for (row, dst) in [(first, &mut *top), (second, &mut *bottom)] {
                for pair in 0..2 {
                    let x = 2 * x + pair;
                    let y = f32::from(row[x]) * (y_scale / 255.0) + y_offset;
                    dst[x * 4..][..4].copy_from_slice(&[
                        encode(y + r),
                        encode(y + g),
                        encode(y + b),
                        255,
                    ]);
                }
            }
        }
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_bundled_avc_with_an_initial_empty_edit() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/graphics/visual_styles/srpg10/background_video.mp4");
        let mut video = Video::open(&path, false).unwrap().unwrap();
        assert_eq!((video.info.width, video.info.height), (1920, 1080));
        assert_eq!(video.info.fps, 30.0);
        assert_eq!(video.info.duration_sec, Some(21.4));
        let picture = video.poster().unwrap();
        assert_eq!(picture.dimensions(), (1920, 1080));
        let mut frames = 0;
        let complete = video
            .decode_one_pass(
                || false,
                |_, ticks| {
                    frames += ticks;
                    Ok(frames < 3)
                },
            )
            .unwrap();
        assert!(!complete);
        assert_eq!(frames, 3);
    }

    #[test]
    fn decodes_indexed_hevc_and_fragmented_vp9() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        for (source, expected) in [
            ("deadlib-video-hevc/tests/fixtures/main.mp4", 12),
            ("deadlib-video-vp9/tests/fixtures/fragmented.mp4", 9),
        ] {
            let mut video = Video::open(&crates.join(source), false).unwrap().unwrap();
            assert_eq!(video.poster().unwrap().dimensions(), (82, 50));
            let info = video.info;
            let mut bytes = vec![0; crate::yuv420_frame_bytes(info)];
            let mut frames = 0;
            assert!(
                video
                    .decode_one_pass(
                        || false,
                        |picture, ticks| {
                            pack(picture, info, &mut bytes)?;
                            frames += ticks;
                            Ok(true)
                        }
                    )
                    .unwrap()
            );
            assert_eq!(frames, expected, "{source}");
        }
    }

    #[test]
    fn decodes_legacy_intro_and_avi_logo() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/media/openitg");
        for (name, fps, duration) in [("intro.mpg", 25.0, 941.0 / 30.0), ("logo.mpg", 30.0, 30.0)] {
            let path = assets.join(name);
            let mut video = Video::open(&path, false).unwrap().unwrap();
            assert_eq!(video.info.fps, fps);
            assert_eq!(video.info.duration_sec, Some(duration));
            assert_eq!(video.poster().unwrap().dimensions(), (320, 240));
            let crate::LoadedVideo {
                info,
                poster,
                mut player,
            } = crate::open(&path, false).unwrap();
            assert_eq!(info.fps, fps);
            assert_eq!(poster.dimensions(), (320, 240));
            let first = player
                .frame_rx
                .as_ref()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            assert_eq!(first.pts_sec, 0.0);
            assert_eq!(first.image.as_raw().len(), 115_200);
            player.stop();
        }
    }

    #[test]
    fn native_player_loops_with_continuous_frame_timestamps() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../deadlib-video-vp9/tests/fixtures/fragmented.mp4");
        let player = crate::open_player(&path, true).unwrap();
        let frames = player.frame_rx.as_ref().unwrap();
        let timeout = std::time::Duration::from_secs(5);
        let first = frames.recv_timeout(timeout).unwrap();
        assert_eq!(first.pts_sec, 0.0);
        for _ in 1..9 {
            let frame = frames.recv_timeout(timeout).unwrap();
            crate::recycle_frame_buffer(&player.recycle_tx, frame.image);
        }
        let repeated = frames.recv_timeout(timeout).unwrap();
        assert!((repeated.pts_sec - 9.0 / player.info.fps).abs() < 0.000_001);
        assert_eq!(first.image.as_raw(), repeated.image.as_raw());
    }
}

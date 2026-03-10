use lewton::inside_ogg::OggStreamReader;
use memmap2::Mmap;
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::Path;

pub(crate) struct OpenFile {
    pub reader: OggStreamReader<BufReader<File>>,
    pub channels: usize,
    pub sample_rate_hz: u32,
}

#[inline(always)]
pub(crate) fn path_is_ogg_vorbis(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "ogg" | "oga"))
}

pub(crate) fn open_file(path: &Path) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(path)?;
    let reader = OggStreamReader::new(BufReader::new(file))?;
    Ok(OpenFile {
        channels: reader.ident_hdr.audio_channels as usize,
        sample_rate_hz: reader.ident_hdr.audio_sample_rate,
        reader,
    })
}

pub(crate) fn file_length_seconds(path: &Path) -> Result<f32, String> {
    let file = File::open(path).map_err(|e| format!("Cannot open file: {e}"))?;
    let mmap = unsafe { Mmap::map(&file) }.map_err(|e| format!("Memory-map failed: {e}"))?;
    let sample_rate_hz = sample_rate_hz(&mmap)?;
    let total_samples = find_last_granule_backwards(&mmap)?;
    Ok((total_samples as f64 / sample_rate_hz) as f32)
}

fn sample_rate_hz(data: &[u8]) -> Result<f64, String> {
    match sample_rate_hz_lewton(data) {
        Ok(rate) => Ok(rate),
        Err(lewton_err) => sample_rate_hz_ident_packet(data).ok_or_else(|| {
            format!("lewton header error: {lewton_err}; fallback OGG ident parse failed")
        }),
    }
}

fn sample_rate_hz_lewton(data: &[u8]) -> Result<f64, String> {
    let cursor = Cursor::new(data);
    let reader = OggStreamReader::new(BufReader::new(cursor)).map_err(|e| format!("{e}"))?;
    let rate = reader.ident_hdr.audio_sample_rate;
    if rate == 0 {
        return Err("Invalid sample rate (0)".into());
    }
    Ok(f64::from(rate))
}

fn sample_rate_hz_ident_packet(data: &[u8]) -> Option<f64> {
    const PAGE_HEADER: usize = 27;
    const MIN_RATE_HZ: u32 = 8_000;
    const MAX_RATE_HZ: u32 = 384_000;

    let mut pos = 0usize;
    let mut packet = Vec::with_capacity(64);
    while pos + PAGE_HEADER <= data.len() {
        if &data[pos..pos + 4] != b"OggS" {
            pos += 1;
            continue;
        }
        let seg_count = data[pos + 26] as usize;
        let header_end = pos.checked_add(PAGE_HEADER + seg_count)?;
        if header_end > data.len() {
            return None;
        }
        let seg_table = &data[pos + PAGE_HEADER..header_end];
        let mut body_pos = header_end;
        for &seg_len_u8 in seg_table {
            let seg_len = seg_len_u8 as usize;
            let seg_end = body_pos.checked_add(seg_len)?;
            if seg_end > data.len() {
                return None;
            }
            packet.extend_from_slice(&data[body_pos..seg_end]);
            body_pos = seg_end;
            if seg_len < 255 {
                if packet.len() < 16 || packet[0] != 0x01 || &packet[1..7] != b"vorbis" {
                    return None;
                }
                let rate = u32::from_le_bytes(packet[12..16].try_into().ok()?);
                if !(MIN_RATE_HZ..=MAX_RATE_HZ).contains(&rate) {
                    return None;
                }
                return Some(f64::from(rate));
            }
        }
        pos = body_pos;
    }
    None
}

fn find_last_granule_backwards(data: &[u8]) -> Result<u64, String> {
    const PAGE_HEADER: usize = 27;
    const CHUNK: usize = 64 * 1024;

    let mut pos = data.len();
    let mut best_granule: Option<u64> = None;
    while pos > PAGE_HEADER {
        let start = pos.saturating_sub(CHUNK);
        let chunk = &data[start..pos];
        let mut i = chunk.len().saturating_sub(PAGE_HEADER);
        while i > 0 {
            if &chunk[i..i + 4] == b"OggS" {
                let granule = u64::from_le_bytes(
                    chunk[i + 6..i + 14]
                        .try_into()
                        .map_err(|_| "Failed to read granule position".to_string())?,
                );
                if granule != u64::MAX && best_granule.is_none_or(|prev| granule > prev) {
                    best_granule = Some(granule);
                }
                i = i.saturating_sub(27 + 255 * 255);
            } else {
                i -= 1;
            }
        }
        if best_granule.is_some() {
            break;
        }
        pos = start;
    }
    best_granule.ok_or_else(|| "No valid granule position found".into())
}

use lewton::inside_ogg::OggStreamReader;
use memmap2::Mmap;
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::Path;

const OGG_PAGE_HEADER_LEN: usize = 27;
const OGG_SCAN_CHUNK_BYTES: usize = 64 * 1024;

pub(crate) struct OpenFile {
    pub reader: OggStreamReader<BufReader<File>>,
    pub channels: usize,
    pub sample_rate_hz: u32,
}

#[inline(always)]
pub(crate) fn path_is_ogg_vorbis(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ogg") || ext.eq_ignore_ascii_case("oga"))
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
    // SAFETY: the mapping is read-only, tied to the lifetime of `file`, and this
    // function never mutates the file descriptor while the map is live.
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
    const MIN_RATE_HZ: u32 = 8_000;
    const MAX_RATE_HZ: u32 = 384_000;

    let mut pos = 0usize;
    let mut packet = Vec::with_capacity(64);
    while pos + OGG_PAGE_HEADER_LEN <= data.len() {
        if &data[pos..pos + 4] != b"OggS" {
            pos += 1;
            continue;
        }
        let seg_count = data[pos + 26] as usize;
        let header_end = pos.checked_add(OGG_PAGE_HEADER_LEN + seg_count)?;
        if header_end > data.len() {
            return None;
        }
        let seg_table = &data[pos + OGG_PAGE_HEADER_LEN..header_end];
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
    let mut pos = data.len();
    while pos >= OGG_PAGE_HEADER_LEN {
        let start = pos.saturating_sub(OGG_SCAN_CHUNK_BYTES);
        if let Some(granule) = find_last_chunk_granule(&data[start..pos]) {
            return Ok(granule);
        }
        if start == 0 {
            break;
        }
        pos = start + OGG_PAGE_HEADER_LEN - 1;
    }
    Err("No valid granule position found".into())
}

fn find_last_chunk_granule(chunk: &[u8]) -> Option<u64> {
    if chunk.len() < OGG_PAGE_HEADER_LEN {
        return None;
    }
    let mut i = chunk.len() - OGG_PAGE_HEADER_LEN;
    loop {
        if has_ogg_page_header(chunk, i) {
            let mut granule_bytes = [0; 8];
            granule_bytes.copy_from_slice(&chunk[i + 6..i + 14]);
            let granule = u64::from_le_bytes(granule_bytes);
            if granule != u64::MAX {
                return Some(granule);
            }
        }
        if i == 0 {
            return None;
        }
        i -= 1;
    }
}

#[inline(always)]
fn has_ogg_page_header(chunk: &[u8], at: usize) -> bool {
    &chunk[at..at + 4] == b"OggS" && chunk[at + 4] == 0
}

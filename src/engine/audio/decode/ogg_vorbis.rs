use lewton::inside_ogg::OggStreamReader;
use memmap2::Mmap;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::Path;

const OGG_PAGE_HEADER_LEN: usize = 27;
const OGG_SCAN_CHUNK_BYTES: usize = 64 * 1024;
const OGG_SERIAL_OFFSET: usize = 14;

#[derive(Clone, Copy)]
struct VorbisStreamInfo {
    serial: u32,
    sample_rate_hz: f64,
}

#[derive(Clone, Copy)]
struct OggPageHeader {
    serial: u32,
    granule: u64,
    continued: bool,
    header_end: usize,
    body_end: usize,
}

pub(crate) struct OpenFile {
    pub reader: Reader,
    pub channels: usize,
    pub sample_rate_hz: u32,
}

enum Inner {
    File(OggStreamReader<BufReader<File>>),
    Memory(OggStreamReader<Cursor<Vec<u8>>>),
}

pub(crate) struct Reader {
    inner: Inner,
    channels: usize,
    blocksize_1: u8,
    pending: Option<Vec<i16>>,
    cursor_frames: u64,
}

#[inline(always)]
pub(crate) fn path_is_ogg_vorbis(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ogg") || ext.eq_ignore_ascii_case("oga"))
}

pub(crate) fn open_file(path: &Path) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(path)?;
    match OggStreamReader::new(BufReader::new(file)) {
        Ok(inner) => opened_from_inner(Inner::File(inner)),
        Err(open_err) => {
            let file = File::open(path)
                .map_err(|e| format!("Cannot reopen file after OGG open failure: {e}"))?;
            // SAFETY: the mapping is read-only, tied to the lifetime of `file`, and this
            // function never mutates the file descriptor while the map is live.
            let mmap =
                unsafe { Mmap::map(&file) }.map_err(|e| format!("Memory-map failed: {e}"))?;
            let Some(stream) = vorbis_stream_info(&mmap) else {
                return Err(open_err.into());
            };
            let pages = extract_logical_stream(&mmap, stream.serial).ok_or_else(|| {
                format!(
                    "Could not extract Vorbis logical stream from '{}'",
                    path.display()
                )
            })?;
            let inner = OggStreamReader::new(Cursor::new(pages))?;
            opened_from_inner(Inner::Memory(inner))
        }
    }
}

fn opened_from_inner(inner: Inner) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
    let (channels, sample_rate_hz, blocksize_1) = match &inner {
        Inner::File(reader) => (
            reader.ident_hdr.audio_channels.max(1) as usize,
            reader.ident_hdr.audio_sample_rate,
            reader.ident_hdr.blocksize_1,
        ),
        Inner::Memory(reader) => (
            reader.ident_hdr.audio_channels.max(1) as usize,
            reader.ident_hdr.audio_sample_rate,
            reader.ident_hdr.blocksize_1,
        ),
    };
    Ok(OpenFile {
        reader: Reader {
            inner,
            channels,
            blocksize_1,
            pending: None,
            cursor_frames: 0,
        },
        channels,
        sample_rate_hz,
    })
}

impl Reader {
    pub(crate) fn read_dec_packet_itl(
        &mut self,
    ) -> Result<Option<Vec<i16>>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(packet) = self.pending.take() {
            self.cursor_frames = self
                .cursor_frames
                .saturating_add((packet.len() / self.channels) as u64);
            return Ok(Some(packet));
        }
        let Some(packet) = self.read_packet()? else {
            return Ok(None);
        };
        self.cursor_frames = self
            .cursor_frames
            .saturating_add((packet.len() / self.channels) as u64);
        Ok(Some(packet))
    }

    pub(crate) fn seek_frame(
        &mut self,
        target_frame: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Seek *before* the target so the Vorbis decoder's overlap
        // state (PreviousWindowRight) is fully primed by the time we
        // reach `target_frame`.  After `seek_absgp_pg` the first
        // decoded packet is a warmup that produces 0 output frames;
        // if the target falls inside that warmup gap the audio we
        // deliver would start late.  A preroll of one maximum block
        // guarantees the gap is behind us.
        let preroll_frames = 1u64 << self.blocksize_1;
        let seek_pos = target_frame.saturating_sub(preroll_frames);
        self.seek_absgp_pg(seek_pos)?;
        self.pending = None;

        // After `seek_absgp_pg`, lewton resets its internal `cur_absgp` to
        // None.  It only becomes Some again after we read the last packet on
        // the first page (`last_in_page`).  Stash all packets until that
        // calibration point so we can compute their absolute positions via
        // backward traversal from the page's absgp.
        let mut stashed: Vec<Vec<i16>> = Vec::new();
        let calibrated_absgp: u64;

        loop {
            let Some(pkt) = self.read_packet()? else {
                self.cursor_frames = target_frame;
                return Ok(());
            };
            stashed.push(pkt);
            if let Some(absgp) = self.last_absgp() {
                calibrated_absgp = absgp;
                break;
            }
        }

        // Assign positions to stashed packets by walking backwards from
        // `calibrated_absgp` (the page's end granule).
        let mut positions: Vec<(u64, u64)> = Vec::with_capacity(stashed.len());
        let mut pos = calibrated_absgp;
        for pkt in stashed.iter().rev() {
            let frames = (pkt.len() / self.channels) as u64;
            let start = pos.saturating_sub(frames);
            positions.push((start, frames));
            pos = start;
        }
        positions.reverse();

        let page_start = positions.first().map_or(calibrated_absgp, |&(s, _)| s);
        self.cursor_frames = page_start;

        // Replay stashed packets, discarding/trimming to reach target.
        for (i, &(pkt_start, pkt_frames)) in positions.iter().enumerate() {
            if pkt_frames == 0 {
                continue; // Skip warmup packets with no decoded audio.
            }
            if pkt_start >= target_frame {
                // This packet is entirely at or past the target — stash it
                // for the next read_dec_packet_itl call.
                self.pending = Some(stashed.into_iter().nth(i).unwrap());
                self.cursor_frames = target_frame;
                return Ok(());
            }
            if pkt_start + pkt_frames > target_frame {
                // Target is within this packet — trim leading samples.
                let skip = (target_frame - pkt_start) as usize;
                let drop_samples = skip * self.channels;
                let pkt = &stashed[i];
                if drop_samples < pkt.len() {
                    self.pending = Some(pkt[drop_samples..].to_vec());
                }
                self.cursor_frames = target_frame;
                return Ok(());
            }
            self.cursor_frames = pkt_start + pkt_frames;
        }

        // Target is beyond this page — continue reading forward.
        // From here, lewton's cur_absgp is calibrated and increments
        // correctly with each decoded packet.
        while self.cursor_frames < target_frame {
            let Some(pkt) = self.read_packet()? else {
                return Ok(());
            };
            let pkt_frames = (pkt.len() / self.channels) as u64;
            let remaining = (target_frame - self.cursor_frames) as usize;
            if remaining < pkt_frames as usize {
                let drop_samples = remaining * self.channels;
                if drop_samples < pkt.len() {
                    self.pending = Some(pkt[drop_samples..].to_vec());
                }
                self.cursor_frames = target_frame;
                return Ok(());
            }
            self.cursor_frames += pkt_frames;
        }

        Ok(())
    }

    #[inline(always)]
    fn read_packet(
        &mut self,
    ) -> Result<Option<Vec<i16>>, Box<dyn std::error::Error + Send + Sync>> {
        match &mut self.inner {
            Inner::File(reader) => Ok(reader.read_dec_packet_itl()?),
            Inner::Memory(reader) => Ok(reader.read_dec_packet_itl()?),
        }
    }

    #[inline(always)]
    fn seek_absgp_pg(
        &mut self,
        target_frame: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match &mut self.inner {
            Inner::File(reader) => Ok(reader.seek_absgp_pg(target_frame)?),
            Inner::Memory(reader) => Ok(reader.seek_absgp_pg(target_frame)?),
        }
    }

    #[inline(always)]
    fn last_absgp(&self) -> Option<u64> {
        match &self.inner {
            Inner::File(reader) => reader.get_last_absgp(),
            Inner::Memory(reader) => reader.get_last_absgp(),
        }
    }
}

pub(crate) fn file_length_seconds(path: &Path) -> Result<f32, String> {
    let file = File::open(path).map_err(|e| format!("Cannot open file: {e}"))?;
    // SAFETY: the mapping is read-only, tied to the lifetime of `file`, and this
    // function never mutates the file descriptor while the map is live.
    let mmap = unsafe { Mmap::map(&file) }.map_err(|e| format!("Memory-map failed: {e}"))?;
    let stream = vorbis_stream_info(&mmap).ok_or_else(|| match sample_rate_hz_lewton(&mmap) {
        Ok(_) => "fallback OGG ident parse failed".to_string(),
        Err(lewton_err) => {
            format!("lewton header error: {lewton_err}; fallback OGG ident parse failed")
        }
    })?;
    let total_samples = find_last_granule_backwards(&mmap, stream.serial)?;
    Ok((total_samples as f64 / stream.sample_rate_hz) as f32)
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

fn vorbis_stream_info(data: &[u8]) -> Option<VorbisStreamInfo> {
    // OGG files may contain cover art or other logical streams before the
    // Vorbis stream. Track packets per serial and find the first completed
    // Vorbis ident packet instead of assuming the first BOS packet is audio.
    let mut packets: HashMap<u32, Vec<u8>> = HashMap::new();
    let mut pos = 0usize;
    while pos + OGG_PAGE_HEADER_LEN <= data.len() {
        if !has_ogg_page_header(data, pos) {
            pos += 1;
            continue;
        }
        let page = parse_page_header(data, pos)?;
        let seg_table = &data[pos + OGG_PAGE_HEADER_LEN..page.header_end];
        let packet = packets.entry(page.serial).or_default();
        if !page.continued {
            packet.clear();
        }
        let mut body_pos = page.header_end;
        for &seg_len_u8 in seg_table {
            let seg_len = seg_len_u8 as usize;
            let seg_end = body_pos.checked_add(seg_len)?;
            packet.extend_from_slice(&data[body_pos..seg_end]);
            body_pos = seg_end;
            if seg_len < 255 {
                if let Some(sample_rate_hz) = sample_rate_hz_ident_packet(packet) {
                    return Some(VorbisStreamInfo {
                        serial: page.serial,
                        sample_rate_hz,
                    });
                }
                packet.clear();
            }
        }
        pos = page.body_end;
    }
    None
}

fn sample_rate_hz_ident_packet(packet: &[u8]) -> Option<f64> {
    const MIN_RATE_HZ: u32 = 8_000;
    const MAX_RATE_HZ: u32 = 384_000;

    if packet.len() < 16 || packet[0] != 0x01 || &packet[1..7] != b"vorbis" {
        return None;
    }
    let rate = u32::from_le_bytes(packet[12..16].try_into().ok()?);
    if !(MIN_RATE_HZ..=MAX_RATE_HZ).contains(&rate) {
        return None;
    }
    Some(f64::from(rate))
}

fn parse_page_header(data: &[u8], pos: usize) -> Option<OggPageHeader> {
    if pos + OGG_PAGE_HEADER_LEN > data.len() || !has_ogg_page_header(data, pos) {
        return None;
    }
    let seg_count = data[pos + 26] as usize;
    let header_end = pos.checked_add(OGG_PAGE_HEADER_LEN + seg_count)?;
    if header_end > data.len() {
        return None;
    }
    let body_len = data[pos + OGG_PAGE_HEADER_LEN..header_end]
        .iter()
        .map(|&len| len as usize)
        .sum::<usize>();
    let body_end = header_end.checked_add(body_len)?;
    if body_end > data.len() {
        return None;
    }
    let serial = u32::from_le_bytes(
        data[pos + OGG_SERIAL_OFFSET..pos + OGG_SERIAL_OFFSET + 4]
            .try_into()
            .ok()?,
    );
    let granule = u64::from_le_bytes(data[pos + 6..pos + 14].try_into().ok()?);
    Some(OggPageHeader {
        serial,
        granule,
        continued: data[pos + 5] & 0x01 != 0,
        header_end,
        body_end,
    })
}

fn extract_logical_stream(data: &[u8], serial: u32) -> Option<Vec<u8>> {
    let mut pos = 0usize;
    let mut stream = Vec::new();
    while pos + OGG_PAGE_HEADER_LEN <= data.len() {
        if !has_ogg_page_header(data, pos) {
            pos += 1;
            continue;
        }
        let page = parse_page_header(data, pos)?;
        if page.serial == serial {
            stream.extend_from_slice(&data[pos..page.body_end]);
        }
        pos = page.body_end;
    }
    (!stream.is_empty()).then_some(stream)
}

fn find_last_granule_backwards(data: &[u8], serial: u32) -> Result<u64, String> {
    let mut pos = data.len();
    while pos >= OGG_PAGE_HEADER_LEN {
        let start = pos.saturating_sub(OGG_SCAN_CHUNK_BYTES);
        if let Some(granule) = find_last_chunk_granule(&data[start..pos], serial) {
            return Ok(granule);
        }
        if start == 0 {
            break;
        }
        pos = start + OGG_PAGE_HEADER_LEN - 1;
    }
    Err("No valid granule position found for Vorbis stream".into())
}

fn find_last_chunk_granule(chunk: &[u8], serial: u32) -> Option<u64> {
    if chunk.len() < OGG_PAGE_HEADER_LEN {
        return None;
    }
    let serial = serial.to_le_bytes();
    let mut i = chunk.len() - OGG_PAGE_HEADER_LEN;
    loop {
        if has_ogg_page_header(chunk, i)
            && chunk.get(i + OGG_SERIAL_OFFSET..i + OGG_SERIAL_OFFSET + 4)
                == Some(serial.as_slice())
        {
            let page = parse_page_header(chunk, i)?;
            if page.granule != u64::MAX {
                return Some(page.granule);
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

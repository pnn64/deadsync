use crate::{
    EncodeOptions, Error, FrameEncoder, Limits, Metadata, MetadataBlock, StreamInfo, checksum::Md5,
    encode::resize, metadata::SeekPoint, reserve,
};
use std::io::{Seek, SeekFrom, Write};

/// Native streaming encoder. Memory is bounded by the block size and reserved
/// seek-point count; PCM input can be planar or interleaved in arbitrary chunks.
/// Any write error is terminal. Finalization consumes the encoder.
pub struct Encoder<W> {
    output: W,
    options: EncodeOptions,
    frame_encoder: FrameEncoder,
    pcm: Vec<i32>,
    pending: usize,
    samples: u64,
    frames: u64,
    bytes: u64,
    audio_offset: u64,
    info: StreamInfo,
    digest: Md5,
    seek_slots: usize,
    seek_points: Vec<SeekPoint>,
    seek_stride: u64,
    failed: bool,
}

pub struct EncodedStream<W> {
    pub output: W,
    /// Final statistics and digest, including when the output was not seekable.
    pub stream_info: StreamInfo,
    pub seek_points: Vec<SeekPoint>,
    pub bytes: u64,
    /// True when finish_seekable patched STREAMINFO and the reserved seek table.
    pub metadata_finalized: bool,
}
impl<W: Write> Encoder<W> {
    pub fn new(output: W, options: EncodeOptions) -> Result<Self, Error> {
        Self::with_metadata(output, options, &[], 0)
    }
    /// Reserve up to `seek_slots` points, distributed across the stream by
    /// progressive thinning. Only finish_seekable installs their offsets.
    /// STREAMINFO and SEEKTABLE are generated; other metadata is preserved.
    pub fn with_metadata(
        mut output: W,
        options: EncodeOptions,
        metadata: &[MetadataBlock<'_>],
        seek_slots: u16,
    ) -> Result<Self, Error> {
        let info = options.stream_info()?;
        if metadata.iter().any(|m| matches!(m.kind, 0 | 3)) {
            return Err(Error::Invalid("encoder owns STREAMINFO and SEEKTABLE"));
        }
        let mut header = b"fLaC".to_vec();
        MetadataBlock {
            kind: 0,
            data: &info.encode()?,
        }
        .write(seek_slots == 0 && metadata.is_empty(), &mut header)?;
        if seek_slots != 0 {
            let mut table = Vec::new();
            reserve(&mut table, usize::from(seek_slots) * 18)?;
            for _ in 0..seek_slots {
                table.extend_from_slice(&u64::MAX.to_be_bytes());
                table.extend_from_slice(&[0; 10]);
            }
            MetadataBlock {
                kind: 3,
                data: &table,
            }
            .write(metadata.is_empty(), &mut header)?;
        }
        for (i, block) in metadata.iter().enumerate() {
            block.write(i + 1 == metadata.len(), &mut header)?;
        }
        // Apply the same structural and singleton validation as native input.
        Metadata::parse(
            &header,
            Limits {
                metadata_bytes: usize::MAX,
                metadata_blocks: usize::MAX,
                ..Limits::default()
            },
        )?;
        let mut pcm = Vec::new();
        resize(
            &mut pcm,
            usize::from(options.channels) * usize::from(options.block_size),
            0,
        )?;
        let mut seek_points = Vec::new();
        reserve(&mut seek_points, seek_slots as usize)?;
        output.write_all(&header)?;
        Ok(Self {
            output,
            options,
            frame_encoder: FrameEncoder::new(options)?,
            pcm,
            pending: 0,
            samples: 0,
            frames: 0,
            bytes: header.len() as u64,
            audio_offset: header.len() as u64,
            info,
            digest: Md5::new(),
            seek_slots: seek_slots as usize,
            seek_points,
            seek_stride: 1,
            failed: false,
        })
    }
    pub fn samples_written(&self) -> u64 {
        self.samples + self.pending as u64
    }
    pub fn write_interleaved(&mut self, input: &[i32]) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("encoder is terminal after error"));
        }
        let result = self.interleaved_inner(input);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn interleaved_inner(&mut self, input: &[i32]) -> Result<(), Error> {
        let channels = self.options.channels as usize;
        if !input.len().is_multiple_of(channels) {
            return Err(Error::Invalid("partial interleaved sample"));
        }
        self.check_input(input.len() / channels, input.iter().copied())?;
        let block = self.options.block_size as usize;
        for chunk in input.chunks_exact(channels) {
            for (ch, &sample) in chunk.iter().enumerate() {
                self.pcm[ch * block + self.pending] = sample;
            }
            self.pending += 1;
            if self.pending == block {
                self.emit()?;
            }
        }
        Ok(())
    }
    pub fn write_planar(&mut self, planes: &[&[i32]]) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("encoder is terminal after error"));
        }
        let result = self.planar_inner(planes);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn planar_inner(&mut self, planes: &[&[i32]]) -> Result<(), Error> {
        if planes.len() != self.options.channels as usize {
            return Err(Error::Invalid("encoder channel count"));
        }
        let samples = planes[0].len();
        if planes.iter().any(|p| p.len() != samples) {
            return Err(Error::Invalid("unequal planar lengths"));
        }
        self.check_input(samples, planes.iter().flat_map(|p| p.iter().copied()))?;
        let block = self.options.block_size as usize;
        let mut pos = 0;
        while pos < samples {
            let take = (block - self.pending).min(samples - pos);
            for (ch, plane) in planes.iter().enumerate() {
                self.pcm[ch * block + self.pending..ch * block + self.pending + take]
                    .copy_from_slice(&plane[pos..pos + take]);
            }
            self.pending += take;
            pos += take;
            if self.pending == block {
                self.emit()?;
            }
        }
        Ok(())
    }
    fn check_input(&self, count: usize, samples: impl Iterator<Item = i32>) -> Result<(), Error> {
        if count as u64 > ((1u64 << 36) - 1).saturating_sub(self.samples_written()) {
            return Err(Error::Invalid("encoder sample count exceeds 36 bits"));
        }
        let bound = 1i64 << (self.options.bits_per_sample - 1);
        if samples
            .into_iter()
            .any(|x| i64::from(x) < -bound || i64::from(x) >= bound)
        {
            return Err(Error::Invalid("PCM outside encoder bit depth"));
        }
        Ok(())
    }
    fn emit(&mut self) -> Result<(), Error> {
        if self.pending == 0 {
            return Ok(());
        }
        let channels = self.options.channels as usize;
        let block = self.options.block_size as usize;
        let mut planes = [&[][..]; 8];
        for (ch, plane) in planes[..channels].iter_mut().enumerate() {
            *plane = &self.pcm[ch * block..ch * block + self.pending];
        }
        let number = if self.options.variable_block_size {
            self.samples
        } else {
            self.frames
        };
        let frame = self.frame_encoder.encode(&planes[..channels], number)?;
        self.output.write_all(frame)?;
        let frame_bytes = frame.len() as u32;
        if self.info.min_frame_size == 0 {
            self.info.min_frame_size = frame_bytes;
        } else {
            self.info.min_frame_size = self.info.min_frame_size.min(frame_bytes);
        }
        self.info.max_frame_size = self.info.max_frame_size.max(frame_bytes);
        // Batch interleaved, sign-extended little-endian PCM for RFC 9639 MD5.
        let width = usize::from(self.options.bits_per_sample.div_ceil(8));
        let mut buf = [0u8; 4096];
        let mut used = 0;
        for i in 0..self.pending {
            for plane in &planes[..channels] {
                if used + width > buf.len() {
                    self.digest.update(&buf[..used]);
                    used = 0;
                }
                buf[used..used + width].copy_from_slice(&plane[i].to_le_bytes()[..width]);
                used += width;
            }
        }
        self.digest.update(&buf[..used]);
        if self.seek_slots != 0 && self.frames.is_multiple_of(self.seek_stride) {
            if self.seek_points.len() == self.seek_slots && self.seek_slots > 1 {
                let mut dest = 0;
                for i in (0..self.seek_points.len()).step_by(2) {
                    self.seek_points[dest] = self.seek_points[i];
                    dest += 1;
                }
                self.seek_points.truncate(dest);
                self.seek_stride *= 2;
            }
            if self.seek_points.len() < self.seek_slots
                && self.frames.is_multiple_of(self.seek_stride)
            {
                self.seek_points.push(SeekPoint {
                    sample: self.samples,
                    offset: self.bytes - self.audio_offset,
                    samples: self.pending as u16,
                });
            }
        }
        self.bytes += u64::from(frame_bytes);
        self.samples += self.pending as u64;
        self.frames += 1;
        self.pending = 0;
        Ok(())
    }
    fn finish_audio(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("encoder is terminal after error"));
        }
        self.emit()?;
        self.info.total_samples = self.samples;
        self.info.md5 = std::mem::take(&mut self.digest).finalize();
        self.output.flush()?;
        Ok(())
    }
    /// Finish a forward-only stream. Its header retains unknown totals/MD5 and
    /// placeholder seek points. Actual metadata is returned for external storage.
    pub fn finish(mut self) -> Result<EncodedStream<W>, Error> {
        self.finish_audio()?;
        Ok(EncodedStream {
            output: self.output,
            stream_info: self.info,
            seek_points: self.seek_points,
            bytes: self.bytes,
            metadata_finalized: false,
        })
    }
}
impl<W: Write + Seek> Encoder<W> {
    /// Finish, patch the native header, and restore the output position to EOF.
    /// Works at a nonzero initial writer position; does not truncate existing data.
    pub fn finish_seekable(mut self) -> Result<EncodedStream<W>, Error> {
        self.finish_audio()?;
        let end = self.output.stream_position()?;
        let start = end
            .checked_sub(self.bytes)
            .ok_or(Error::Invalid("encoder output position changed"))?;
        self.output.seek(SeekFrom::Start(start + 8))?;
        self.output.write_all(&self.info.encode()?)?;
        if self.seek_slots != 0 {
            self.output.seek(SeekFrom::Start(start + 46))?;
            for point in &self.seek_points {
                self.output.write_all(&point.sample.to_be_bytes())?;
                self.output.write_all(&point.offset.to_be_bytes())?;
                self.output.write_all(&point.samples.to_be_bytes())?;
            }
        }
        self.output.seek(SeekFrom::Start(end))?;
        self.output.flush()?;
        Ok(EncodedStream {
            output: self.output,
            stream_info: self.info,
            seek_points: self.seek_points,
            bytes: self.bytes,
            metadata_finalized: true,
        })
    }
}

use crate::{
    EncodeOptions, Encoder, Error, FrameDecoder, FrameHeader, Limits, Metadata, MetadataBlock,
    StreamInfo,
    checksum::{Md5, crc16},
    ogg::Sequence,
};
use deadlib_audio_decode_ogg::{self as container, PacketWriter, Page};
use std::io::Write;

#[derive(Clone, Copy, Debug, Default)]
pub struct OggWriteOptions {
    pub serial: u32,
    pub granule_offset: u64,
    pub container: container::WriteOptions,
}
pub struct OggEncodedStream<W> {
    pub output: W,
    pub stream_info: StreamInfo,
    pub bytes: u64,
}

/// Packet-level muxer. Public writes validate one complete raw FLAC frame;
/// native OggEncoder uses the same mapping with already-validated encoder output.
pub struct OggMuxer<W> {
    packets: PacketWriter<HeaderPages<W>>,
    info: StreamInfo,
    options: OggWriteOptions,
    sequence: Sequence,
    decoder: FrameDecoder,
    digest: Option<Md5>,
    failed: bool,
}
impl<W: Write> OggMuxer<W> {
    pub fn new(
        output: W,
        info: StreamInfo,
        metadata: &[MetadataBlock<'_>],
        options: OggWriteOptions,
    ) -> Result<Self, Error> {
        info.validate()?;
        if options.granule_offset > i64::MAX as u64 {
            return Err(Error::Invalid("negative Ogg granule origin"));
        }
        if metadata.iter().any(|m| m.kind == 0) {
            return Err(Error::Invalid("duplicate Ogg STREAMINFO"));
        }
        let comment = metadata.iter().position(|m| m.kind == 4);
        let count = metadata.len() + usize::from(comment.is_none());
        if count > u16::MAX as usize {
            return Err(Error::LimitExceeded("Ogg metadata packet count"));
        }
        let vendor = b"deadlib-audio-decode-flac";
        let mut default_comment = (vendor.len() as u32).to_le_bytes().to_vec();
        default_comment.extend_from_slice(vendor);
        default_comment.extend_from_slice(&0u32.to_le_bytes());
        let first = comment.map(|i| metadata[i]).unwrap_or(MetadataBlock {
            kind: 4,
            data: &default_comment,
        });
        let blocks: Vec<_> = std::iter::once(first)
            .chain(
                metadata
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &m)| (Some(i) != comment).then_some(m)),
            )
            .collect();
        let info_bytes = info.encode()?;
        let mut native = b"fLaC".to_vec();
        MetadataBlock {
            kind: 0,
            data: &info_bytes,
        }
        .write(false, &mut native)?;
        for (i, block) in blocks.iter().enumerate() {
            block.write(i + 1 == blocks.len(), &mut native)?;
        }
        Metadata::parse(
            &native,
            Limits {
                metadata_bytes: usize::MAX,
                metadata_blocks: usize::MAX,
                ..Default::default()
            },
        )?;
        let sink = HeaderPages {
            output,
            remaining: count + 1,
            buffer: Vec::new(),
            bytes: 0,
        };
        let mut packets = PacketWriter::with_options(sink, options.serial, options.container);
        let mut packet = b"\x7fFLAC\x01\x00".to_vec();
        packet.extend_from_slice(&(count as u16).to_be_bytes());
        packet.extend_from_slice(&native[..42]);
        packets.write_packet(&packet, 0, false)?;
        for (i, block) in blocks.iter().enumerate() {
            packet.clear();
            block.write(i + 1 == blocks.len(), &mut packet)?;
            packets.write_packet(&packet, 0, false)?;
        }
        packets.flush_page()?;
        Ok(Self {
            packets,
            info,
            options,
            sequence: Sequence::new(),
            decoder: FrameDecoder::new(Some(info))?,
            digest: (info.md5 != [0; 16]).then(Md5::new),
            failed: false,
        })
    }
    pub fn write_frame(&mut self, data: &[u8]) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("Ogg muxer is terminal after error"));
        }
        let result = (|| {
            let frame = self.decoder.decode(data)?;
            if frame.bytes != data.len() {
                return Err(Error::Invalid("one FLAC frame per Ogg packet"));
            }
            if let Some(digest) = &mut self.digest {
                frame.pcm.update_md5(digest, frame.header.bits_per_sample);
            }
            let header = frame.header;
            self.write_header_checked(data, header)
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn write_generated(&mut self, data: &[u8]) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("Ogg muxer is terminal after error"));
        }
        let result = (|| {
            let header = FrameHeader::parse(data, Some(&self.info))?;
            if crc16(data) != 0 {
                return Err(Error::Checksum("generated frame CRC-16"));
            }
            self.write_header_checked(data, header)
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn write_header_checked(&mut self, data: &[u8], header: FrameHeader) -> Result<(), Error> {
        self.sequence.advance(header, self.info, true)?;
        let granule = self
            .options
            .granule_offset
            .checked_add(self.sequence.samples)
            .filter(|&n| n <= i64::MAX as u64)
            .ok_or(Error::Invalid("Ogg granule overflow"))?;
        self.packets.write_packet(data, granule, false)?;
        Ok(())
    }
    pub fn finish(mut self) -> Result<OggEncodedStream<W>, Error> {
        if self.failed {
            return Err(Error::Invalid("Ogg muxer is terminal after error"));
        }
        self.sequence.finish(self.info)?;
        if self.sequence.base == 0
            && let Some(digest) = self.digest.take()
            && digest.finalize() != self.info.md5
        {
            return Err(Error::Checksum("PCM MD5"));
        }
        self.packets.finish()?;
        let mut sink = self.packets.into_inner();
        sink.output.flush()?;
        Ok(OggEncodedStream {
            output: sink.output,
            stream_info: self.info,
            bytes: sink.bytes,
        })
    }
}

/// Correct generic Ogg pagination's unfinished-header granules to RFC 9639's
/// metadata-page zero. Audio writes pass through directly after the header phase.
struct HeaderPages<W> {
    output: W,
    remaining: usize,
    buffer: Vec<u8>,
    bytes: u64,
}
impl<W: Write> Write for HeaderPages<W> {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            let n = self.output.write(input)?;
            self.bytes += n as u64;
            return Ok(n);
        }
        self.buffer
            .try_reserve(input.len())
            .map_err(std::io::Error::other)?;
        self.buffer.extend_from_slice(input);
        loop {
            let (page, used) = match Page::parse(&self.buffer) {
                Ok(page) => page,
                Err(container::Error::Truncated) => break,
                Err(e) => return Err(std::io::Error::other(e)),
            };
            if page.completed_packets() > self.remaining {
                return Err(std::io::Error::other("audio on metadata page"));
            }
            let mut header = page.header();
            header.granule_position = 0;
            container::write_page(&mut self.output, header, page.lacing(), page.body())
                .map_err(std::io::Error::other)?;
            self.remaining -= page.completed_packets();
            self.bytes += used as u64;
            self.buffer.copy_within(used.., 0);
            self.buffer.truncate(self.buffer.len() - used);
            if self.remaining == 0 {
                self.output.write_all(&self.buffer)?;
                self.bytes += self.buffer.len() as u64;
                self.buffer = Vec::new();
                break;
            }
        }
        Ok(input.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.output.flush()
    }
}

// The native streaming encoder emits a complete header in its initial write,
// then one complete encoded frame per write. This private sink shares its PCM
// scheduling, precision validation and MD5 implementation without collecting a file.
struct NativePackets<W> {
    output: Option<W>,
    muxer: Option<OggMuxer<W>>,
    options: OggWriteOptions,
}
impl<W: Write> Write for NativePackets<W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let result = (|| -> Result<(), Error> {
            if let Some(muxer) = &mut self.muxer {
                muxer.write_generated(data)?;
            } else {
                let metadata = Metadata::parse(
                    data,
                    Limits {
                        metadata_bytes: usize::MAX,
                        metadata_blocks: usize::MAX,
                        ..Default::default()
                    },
                )?;
                if metadata.audio_offset != data.len() {
                    return Err(Error::Invalid("native encoder header write boundary"));
                }
                self.muxer = Some(OggMuxer::new(
                    self.output
                        .take()
                        .ok_or(Error::Invalid("missing Ogg writer"))?,
                    metadata.stream_info,
                    &metadata.blocks[1..],
                    self.options,
                )?);
            }
            Ok(())
        })();
        result.map_err(std::io::Error::other)?;
        Ok(data.len())
    }
    // OggEncoder::finish flushes the final EOS page and the underlying writer.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub struct OggEncoder<W> {
    encoder: Encoder<NativePackets<W>>,
}
impl<W: Write> OggEncoder<W> {
    pub fn new(output: W, options: EncodeOptions, ogg: OggWriteOptions) -> Result<Self, Error> {
        Self::with_metadata(output, options, &[], ogg)
    }
    pub fn with_metadata(
        output: W,
        options: EncodeOptions,
        metadata: &[MetadataBlock<'_>],
        ogg: OggWriteOptions,
    ) -> Result<Self, Error> {
        Ok(Self {
            encoder: Encoder::with_metadata(
                NativePackets {
                    output: Some(output),
                    muxer: None,
                    options: ogg,
                },
                options,
                metadata,
                0,
            )?,
        })
    }
    pub fn write_planar(&mut self, planes: &[&[i32]]) -> Result<(), Error> {
        self.encoder.write_planar(planes)
    }
    pub fn write_interleaved(&mut self, pcm: &[i32]) -> Result<(), Error> {
        self.encoder.write_interleaved(pcm)
    }
    pub fn samples_written(&self) -> u64 {
        self.encoder.samples_written()
    }
    /// Forward-only Ogg output retains unknown STREAMINFO count/digest. Final
    /// metadata is returned; every frame and page has its checked CRC.
    pub fn finish(self) -> Result<OggEncodedStream<W>, Error> {
        let native = self.encoder.finish()?;
        let mut result = native
            .output
            .muxer
            .ok_or(Error::Invalid("missing Ogg headers"))?
            .finish()?;
        result.stream_info = native.stream_info;
        Ok(result)
    }
}

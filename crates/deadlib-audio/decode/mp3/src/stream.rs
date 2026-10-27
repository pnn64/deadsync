//! Stateful raw PCM from any `Read` source. No file-level gapless trimming.
use crate::{
    DecodeOptions, Error, FrameDecoder, FrameHeader, Pcm,
    reader::{Metadata, PacketReader, ReaderOptions, TruncatedPolicy},
};
use std::io::Read;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Recovery {
    #[default]
    Strict,
    /// Conceal complete damaged frames with silence and retain their raw timing.
    Conceal,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct StreamOptions {
    pub decode: DecodeOptions,
    pub recovery: Recovery,
    pub truncated: TruncatedPolicy,
    pub free_format: Option<crate::FreeFormat>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameStatus {
    #[default]
    Decoded,
    MissingReservoir,
    Corrupt,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct StreamStats {
    pub decoded_frames: u64,
    pub concealed_frames: u64,
    pub raw_samples: u64,
}
pub struct StreamFrame<'a> {
    pub header: FrameHeader,
    pub pcm: Pcm<'a>,
    /// Raw sample position per channel, including metadata frames and DSP delay.
    pub position: u64,
    pub byte_offset: u64,
    pub skipped_bytes: u64,
    pub status: FrameStatus,
}
struct Output {
    header: FrameHeader,
    position: u64,
    byte_offset: u64,
    skipped_bytes: u64,
    status: FrameStatus,
}
pub struct StreamDecoder<R> {
    packets: PacketReader<R>,
    decoder: FrameDecoder,
    options: StreamOptions,
    stats: StreamStats,
    last_error: Option<Error>,
    failed: bool,
}
impl<R: Read> StreamDecoder<R> {
    pub fn new(source: R) -> Self {
        Self::with_options(source, StreamOptions::default())
    }
    pub fn with_options(source: R, options: StreamOptions) -> Self {
        Self {
            packets: PacketReader::with_options(
                source,
                ReaderOptions {
                    limits: options.decode.limits,
                    truncated: options.truncated,
                    free_format: options.free_format,
                },
            ),
            decoder: FrameDecoder::with_options(options.decode),
            options,
            stats: StreamStats::default(),
            last_error: None,
            failed: false,
        }
    }
    pub fn metadata(&self) -> &Metadata {
        self.packets.metadata()
    }
    pub fn stats(&self) -> StreamStats {
        self.stats
    }
    pub fn last_recovered_error(&self) -> Option<&Error> {
        self.last_error.as_ref()
    }
    pub fn skipped_bytes(&self) -> u64 {
        self.packets.skipped_bytes()
    }
    pub fn discarded_tail_bytes(&self) -> usize {
        self.packets.discarded_tail_bytes()
    }
    pub fn into_parts(self) -> (R, Vec<u8>) {
        self.packets.into_parts()
    }
    pub fn reset(&mut self, source: R) {
        self.packets.reset(source);
        self.decoder.reset();
        self.stats = StreamStats::default();
        self.last_error = None;
        self.failed = false;
    }
    fn advance(&mut self) -> Result<Option<Output>, Error> {
        let Some(packet) = self.packets.next_packet()? else {
            return Ok(None);
        };
        if packet.skipped_bytes != 0 && self.stats.decoded_frames + self.stats.concealed_frames != 0
        {
            if self.options.recovery == Recovery::Strict {
                return Err(Error::Invalid("lost stream synchronization"));
            }
            self.decoder.reset();
        }
        let mut status = FrameStatus::Decoded;
        if let Err(error) = self.decoder.decode(packet.data, packet.free_format) {
            if self.options.recovery == Recovery::Strict
                || matches!(
                    error,
                    Error::Unsupported(_)
                        | Error::LimitExceeded(_)
                        | Error::Allocation
                        | Error::Io(_)
                )
            {
                return Err(error);
            }
            let missing = matches!(error, Error::MissingReservoir { .. });
            self.decoder
                .conceal(&packet.data[packet.header.main_data_offset()..])?;
            status = if missing {
                FrameStatus::MissingReservoir
            } else {
                FrameStatus::Corrupt
            };
            self.last_error = Some(error);
        }
        if status == FrameStatus::Decoded {
            self.stats.decoded_frames += 1;
        } else {
            self.stats.concealed_frames += 1;
        }
        let position = self.stats.raw_samples;
        self.stats.raw_samples = position
            .checked_add(packet.header.samples() as u64)
            .ok_or(Error::LimitExceeded("stream sample position"))?;
        Ok(Some(Output {
            header: packet.header,
            position,
            byte_offset: packet.byte_offset,
            skipped_bytes: packet.skipped_bytes,
            status,
        }))
    }
    pub fn next_frame(&mut self) -> Result<Option<StreamFrame<'_>>, Error> {
        if self.failed {
            return Err(Error::Invalid("stream decoder requires reset after error"));
        }
        match self.advance() {
            Ok(Some(out)) => Ok(Some(StreamFrame {
                header: out.header,
                pcm: self.decoder.pcm(out.header),
                position: out.position,
                byte_offset: out.byte_offset,
                skipped_bytes: out.skipped_bytes,
                status: out.status,
            })),
            Ok(None) => Ok(None),
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }
}

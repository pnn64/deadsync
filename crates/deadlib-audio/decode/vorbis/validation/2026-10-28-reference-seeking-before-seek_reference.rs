//! Reference file scheduling over the independently validated native page index.
//!
//! The scheduling rules follow Xiph libvorbis 1.3.7 vorbisfile.c (BSD-3-Clause,
//! Copyright 1994-2015 Xiph.Org Foundation). Codec/DSP and container parsing are
//! the native Rust implementations; no reference code is linked at runtime.
//! See COPYING.Xiph for the reference license and disclaimer.
use super::*;
use crate::{PacketTiming, SynthesisDecoder, bitpack::BitReader};

pub(super) struct ReferenceSeek {
    decoder: SynthesisDecoder,
    packet: Vec<u8>,
    timing: PacketTiming,
    available: bool,
    number: u64,
}
impl ReferenceSeek {
    pub(super) fn storage_bytes(&self) -> usize {
        self.decoder.setup().storage_bytes() + self.decoder.storage_bytes() + self.packet.capacity()
    }
    fn block_size(&self) -> Option<usize> {
        let mut bits = BitReader::new(&self.packet);
        if bits.read(1).ok()? != 0 {
            return None;
        }
        let modes = self.decoder.setup().modes();
        let width = usize::BITS - (modes.len() - 1).leading_zeros();
        let mode = modes.get(bits.read(width).ok()? as usize)?;
        Some(self.decoder.setup().block_sizes()[mode.long_block as usize])
    }
}

impl<R: Read + Seek> OggSeekReader<R> {
    pub(super) fn seek_reference(&mut self, selected: usize, frame: u64) -> Result<(), Error> {
        let stream = &self.index.streams[selected];
        let options = self.index.options;
        if options.trim_to_final_granule {
            return Err(Error::Invalid("reference seeking with endpoint repair"));
        }
        if self.selected != Some(selected) || self.reference.is_none() {
            self.reference = None;
            let mut limits = options.limits;
            limits.max_setup_memory = limits.max_setup_memory.min(options.max_stream_memory);
            let setup = Setup::parse_with_options(
                &stream.setup_packet,
                &stream.info.identification,
                ParseOptions {
                    limits,
                    allow_empty_codebooks: options.allow_empty_codebooks,
                    codebook_behavior: options.codebook_behavior,
                    residue_behavior: options.residue_behavior,
                },
            )?;
            check_decoder_memory(&setup, options)?;
            self.reference = Some(ReferenceSeek {
                decoder: SynthesisDecoder::with_options(
                    setup,
                    DecodeOptions {
                        limits,
                        residue2_layout: options.residue2_layout,
                        floor0_behavior: options.floor0_behavior,
                        floor1_behavior: options.floor1_behavior,
                        allow_nonfinite: options.allow_nonfinite,
                    },
                )?,
                packet: Vec::new(),
                timing: PacketTiming::default(),
                available: false,
                number: 0,
            });
        } else if let Some(reference) = &mut self.reference {
            reference.decoder.reset();
        }
        self.selected = Some(selected);
        self.finished = false;
        self.announce = true;
        self.page_pending = false;
        let entries = self.index.container.pages(stream.info.id.serial).unwrap();
        // vorbisfile's data offset is immediately after the setup-header page.
        let setup_page = entries
            .iter()
            .position(|p| p.first_packet + p.completed_packets as u64 > 2)
            .ok_or(Error::Invalid("missing setup page"))?;
        let first = setup_page + 1;
        let target = frame
            .checked_add(stream.end.start_granule)
            .ok_or(Error::LimitExceeded("seek granule"))?;
        let mut best = None;
        let mut observed = first;
        for (i, entry) in entries.iter().enumerate().skip(first) {
            // The final physical page starts the last link's search end fence.
            // Its granule may be a decreasing/malformed EOS, never a candidate.
            if i + 1 == entries.len() && i != first {
                break;
            }
            observed = i;
            let gp = entry.header.granule_position;
            if gp == UNKNOWN_GRANULE_POSITION {
                continue;
            }
            if gp >= target {
                break;
            }
            best = Some(i);
        }
        let anchor = best.unwrap_or(observed);
        let entry = entries
            .get(anchor)
            .ok_or(Error::Invalid("missing audio page"))?;
        self.next_page = anchor;
        self.capture.reset_at(entry);
        self.pages.seek(entry.offset)?;
        let reference = self.reference.as_mut().unwrap();
        reference.packet.clear();
        reference.available = false;
        reference.number = 0;
        self.position = 0;
        if best.is_some() {
            // A continued page can carry a granule but no independently
            // available packet. vorbisfile then backs up and uses raw-seek's
            // two-queue scan instead of borrowing a later page's landmark.
            let anchor_end = entry.offset + entry.length as u64;
            let available = self.reference_peek()?;
            let on_anchor = self.pages.position() == anchor_end;
            if !available || !on_anchor {
                let entries = self
                    .index
                    .container
                    .pages(self.index.streams[selected].info.id.serial)
                    .unwrap();
                let mut start = anchor;
                while start > first {
                    start -= 1;
                    let page = &entries[start];
                    if page.header.granule_position != UNKNOWN_GRANULE_POSITION
                        || !page.header.is_continued()
                    {
                        break;
                    }
                }
                self.reference_raw_seek(start)?;
            } else {
                loop {
                    let reference = self.reference.as_mut().unwrap();
                    if let Some(gp) = reference.timing.granule_position {
                        self.position = (gp as u64)
                            .saturating_sub(self.index.streams[selected].end.start_granule);
                        break;
                    }
                    reference.available = false;
                    if !self.reference_peek()? {
                        return Err(Error::Invalid("reference seek page has no packet"));
                    }
                }
            }
        }
        let mut last_block = 0;
        while self.reference_peek()? {
            let reference = self.reference.as_mut().unwrap();
            let Some(block) = reference.block_size() else {
                reference.available = false;
                continue;
            };
            if last_block != 0 {
                self.position = self
                    .position
                    .checked_add(((last_block + block) / 4) as u64)
                    .ok_or(Error::LimitExceeded("seek position"))?;
            }
            let maximum = reference.decoder.setup().block_sizes()[1];
            if self.position.saturating_add(((block + maximum) / 4) as u64) >= frame {
                break;
            }
            reference.available = false;
            reference
                .decoder
                .track(&reference.packet, reference.timing)?;
            if let Some(gp) = reference.timing.granule_position {
                self.position =
                    (gp as u64).saturating_sub(self.index.streams[selected].end.start_granule);
            }
            last_block = block;
        }
        while self.position < frame {
            let reference = self.reference.as_mut().unwrap();
            let wanted = frame - self.position;
            let samples = (reference.decoder.pcm().frames as u64).min(wanted) as usize;
            reference.decoder.consume(samples)?;
            self.position += samples as u64;
            if (samples as u64) < wanted && !self.reference_fetch()? {
                self.position = self.index.streams[selected].end.frames;
                break;
            }
        }
        Ok(())
    }

    fn reference_rewind(&mut self, page: usize) -> Result<(), Error> {
        let stream = &self.index.streams[self.selected.unwrap()];
        let entry = &self.index.container.pages(stream.info.id.serial).unwrap()[page];
        self.capture.reset_at(entry);
        self.pages.seek(entry.offset)?;
        self.next_page = page;
        self.page_pending = false;
        let reference = self.reference.as_mut().unwrap();
        reference.available = false;
        reference.number = 0;
        Ok(())
    }

    fn reference_raw_seek(&mut self, start: usize) -> Result<(), Error> {
        self.reference_rewind(start)?;
        let mut previous = 0;
        let mut accumulated = 0u64;
        let mut dropped = 0;
        loop {
            if !self.reference_peek()? {
                self.position = self.index.streams[self.selected.unwrap()].end.frames;
                return Ok(());
            }
            let reference = self.reference.as_mut().unwrap();
            let block = reference.block_size().unwrap_or(0);
            // raw_seek queues all packets but removes the oldest queued packet
            // for each non-audio packet or packet completed on the final page.
            let last_page = self.pages.current_page().unwrap().page.header().is_eos();
            if block == 0 || last_page {
                dropped += 1;
            } else if previous != 0 {
                accumulated += ((previous + block) / 4) as u64;
            }
            reference.available = false;
            if let Some(gp) = reference.timing.granule_position {
                self.position = (gp as u64)
                    .saturating_sub(self.index.streams[self.selected.unwrap()].end.start_granule)
                    .saturating_sub(accumulated);
                break;
            }
            previous = block;
        }
        self.reference_rewind(start)?;
        for _ in 0..dropped {
            if !self.reference_peek()? {
                return Err(Error::Invalid("raw seek queue"));
            }
            self.reference.as_mut().unwrap().available = false;
        }
        Ok(())
    }

    // Keep exactly one complete packet across a peek. PacketCapture retains the
    // verified page cursor and any spanning fragment, with independent limits.
    fn reference_peek(&mut self) -> Result<bool, Error> {
        let reference = self.reference.as_mut().unwrap();
        if reference.available {
            return Ok(true);
        }
        let stream = &self.index.streams[self.selected.unwrap()];
        let entries = self.index.container.pages(stream.info.id.serial).unwrap();
        while let Some(entry) = entries.get(self.next_page) {
            let page = if self.page_pending {
                self.pages
                    .current_page()
                    .ok_or(Error::Invalid("missing reference seek page"))?
                    .page
            } else {
                read_page(&mut self.pages, entry)?
            };
            let mut failure = None;
            let complete = self.capture.push_page_until(page, entry.offset, |packet| {
                reference.packet.clear();
                let fixed =
                    reference.decoder.setup().storage_bytes() + reference.decoder.storage_bytes();
                let budget = self.index.options.max_stream_memory.saturating_sub(fixed);
                let reserve = if packet.data.len() > budget {
                    Err(Error::LimitExceeded("selected stream memory"))
                } else {
                    reference
                        .packet
                        .try_reserve_exact(packet.data.len())
                        .map_err(|_| Error::Allocation)
                };
                if let Err(error) = reserve {
                    failure = Some(error);
                } else {
                    reference.packet.extend_from_slice(packet.data);
                    reference.timing = PacketTiming {
                        packet_number: reference.number,
                        granule_position: packet.info.granule_position.map(|n| n as i64),
                        end_of_stream: packet.info.last_in_stream,
                    };
                    reference.number = reference.number.wrapping_add(1);
                    reference.available = true;
                }
                ControlFlow::Break(())
            })?;
            self.page_pending = !complete;
            if complete {
                self.next_page += 1;
            }
            if let Some(error) = failure {
                return Err(error);
            }
            if reference.available {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn reference_fetch(&mut self) -> Result<bool, Error> {
        while self.reference_peek()? {
            let reference = self.reference.as_mut().unwrap();
            reference.available = false;
            if reference
                .decoder
                .submit(&reference.packet, reference.timing)?
            {
                if let Some(gp) = reference.timing.granule_position
                    && !reference.timing.end_of_stream
                {
                    // libvorbisfile subtracts the initial offset on later links
                    // only; the first link intentionally preserves its quirk.
                    let origin = if self.selected == Some(0) {
                        0
                    } else {
                        self.index.streams[self.selected.unwrap()].end.start_granule
                    };
                    self.position = (gp as u64)
                        .saturating_sub(origin)
                        .saturating_sub(reference.decoder.pcm().frames as u64);
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn read_reference(
        &mut self,
        emit: &mut impl FnMut(OggEvent<'_>),
    ) -> Result<bool, Error> {
        let selected = self.selected.unwrap();
        let stream = &self.index.streams[selected];
        if self.announce {
            if let Some(error) = &stream.comment_error {
                emit(OggEvent::CommentError {
                    stream: stream.info.id,
                    error: error.clone(),
                });
            }
            emit(OggEvent::StreamStart(&stream.info));
            self.announce = false;
        }
        while self.reference.as_ref().unwrap().decoder.pcm().frames == 0 {
            if !self.reference_fetch()? {
                emit(OggEvent::StreamEnd(self.index.streams[selected].end));
                self.finished = true;
                return Ok(true);
            }
        }
        let stream = &self.index.streams[selected];
        let decoder = &mut self.reference.as_mut().unwrap().decoder;
        let pcm = decoder.pcm();
        let frames = pcm.frames;
        emit(OggEvent::Pcm(OggPcm {
            stream: stream.info.id,
            sample_rate: stream.info.identification.sample_rate,
            start_frame: self.position,
            start_granule: self.position.saturating_add(stream.end.start_granule),
            pcm,
        }));
        decoder.consume(frames)?;
        self.position = self
            .position
            .checked_add(frames as u64)
            .ok_or(Error::LimitExceeded("seek position"))?;
        Ok(true)
    }
}

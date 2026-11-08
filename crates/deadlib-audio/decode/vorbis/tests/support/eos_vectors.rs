//! EOF layouts preserve fixture payloads; independent C tools supply PCM truth.
use super::codec::*;
use deadlib_audio_decode_ogg::{self as ogg, Demuxer, PacketWriter, PageHeader, PageReader};
use std::io::Cursor;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tail {
    Audio,
    Nil,
    Empty,
    NonAudio,
    Gap,
}
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub fixture: usize,
    pub group: usize,
    pub tail: Tail,
    pub count: usize,
    pub end: u64,
}
pub const FILES: &[&[u8]] = &[
    include_bytes!("../fixtures/stereo.ogg"),
    include_bytes!("../fixtures/mono.ogg"),
    include_bytes!("../fixtures/surround.ogg"),
    include_bytes!("../fixtures/eight-channel.ogg"),
];
pub fn packets(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut pages = PageReader::new(Cursor::new(bytes));
    let mut demux = Demuxer::with_validation(
        ogg::Limits::default(),
        ogg::ValidationOptions {
            allow_decreasing_eos_granule: true,
            ..ogg::ValidationOptions::default()
        },
    );
    let mut out = Vec::new();
    while let Some(p) = pages.next_page().unwrap() {
        demux
            .push_page(p.page, p.offset, |p| out.push(p.data.to_vec()))
            .unwrap();
    }
    demux.finish().unwrap();
    out
}
pub fn raw(bytes: &[u8]) -> (Vec<Vec<u8>>, Vec<u64>) {
    let p = packets(bytes);
    let id = Identification::parse(&p[0]).unwrap();
    let setup = Setup::parse(&p[2], &id, &Limits::default()).unwrap();
    let mut d = PacketDecoder::new(setup).unwrap();
    let mut ends = Vec::new();
    for p in &p[3..] {
        let b = d.decode(p).unwrap().unwrap();
        ends.push(b.start_frame + b.frames as u64);
    }
    (p, ends)
}
impl Config {
    pub fn bytes(self) -> Vec<u8> {
        let (p, ends) = raw(FILES[self.fixture]);
        let count = self.count.min(ends.len());
        let mut audio: Vec<_> = p[3..3 + count]
            .iter()
            .cloned()
            .zip(ends[..count].iter().copied())
            .collect();
        if self.tail == Tail::Gap && count > 0 {
            audio.insert(
                count - 1,
                (vec![1], if count >= 2 { ends[count - 2] } else { 0 }),
            );
        }
        if self.tail == Tail::Empty {
            audio.push((
                vec![],
                ends.get(count.wrapping_sub(1)).copied().unwrap_or(0),
            ));
        }
        if self.tail == Tail::NonAudio {
            audio.push((
                vec![1],
                ends.get(count.wrapping_sub(1)).copied().unwrap_or(0),
            ));
        }
        let mut w = PacketWriter::new(Vec::new(), 193 + self.fixture as u32);
        for h in &p[..3] {
            w.write_packet(h, 0, false).unwrap();
        }
        w.flush_page().unwrap();
        for (i, (p, end)) in audio.iter().enumerate() {
            let last = i + 1 == audio.len();
            let eos = last && self.tail != Tail::Nil;
            w.write_packet(p, if eos { self.end } else { *end }, eos)
                .unwrap();
            if (i + 1).is_multiple_of(self.group.max(1)) {
                w.flush_page().unwrap();
            }
        }
        if self.tail == Tail::Nil || audio.is_empty() {
            w.flush_page().unwrap();
            let mut b = w.into_inner();
            let mut pages = PageReader::new(Cursor::new(&b));
            let mut h = None;
            while let Some(p) = pages.next_page().unwrap() {
                h = Some(p.page.header());
            }
            let h = h.unwrap();
            b.extend(
                ogg::encode_page(
                    PageHeader {
                        flags: ogg::EOS,
                        granule_position: self.end,
                        serial: h.serial,
                        sequence: h.sequence + 1,
                    },
                    &[],
                    &[],
                )
                .unwrap(),
            );
            b
        } else {
            w.into_inner()
        }
    }
}
#[derive(Clone, Debug)]
pub struct Record {
    pub data: Vec<u8>,
    pub gp: Option<u64>,
    pub eos: bool,
    pub page: PageHeader,
    pub ends_packet: bool,
}
pub fn timed(bytes: &[u8]) -> (Vec<Vec<u8>>, Vec<Record>) {
    let mut pages = PageReader::new(Cursor::new(bytes));
    let mut demux = Demuxer::with_validation(
        ogg::Limits::default(),
        ogg::ValidationOptions {
            allow_decreasing_eos_granule: true,
            ..ogg::ValidationOptions::default()
        },
    );
    let mut headers = Vec::new();
    let mut out = Vec::new();
    while let Some(p) = pages.next_page().unwrap() {
        let h = p.page.header();
        let ends_packet = p.page.lacing().last().is_none_or(|&n| n < 255);
        demux
            .push_page(p.page, p.offset, |p| {
                if p.info.number < 3 {
                    headers.push(p.data.to_vec());
                } else {
                    out.push(Record {
                        data: p.data.to_vec(),
                        gp: p.info.granule_position,
                        eos: h.is_eos() && p.info.granule_position.is_some(),
                        page: h,
                        ends_packet,
                    });
                }
            })
            .unwrap();
    }
    demux.finish().unwrap();
    (headers, out)
}
pub fn request(headers: &[Vec<u8>], cases: &[Vec<Record>]) -> Vec<u8> {
    let mut out = Vec::new();
    for h in headers {
        out.extend((h.len() as u64).to_le_bytes());
        out.extend(h);
    }
    out.extend((cases.len() as u64).to_le_bytes());
    for records in cases {
        out.extend((records.len() as u64).to_le_bytes());
        for r in records {
            out.extend(r.gp.unwrap_or(u64::MAX).to_le_bytes());
            out.extend((r.eos as u64).to_le_bytes());
            out.extend((r.data.len() as u64).to_le_bytes());
            out.extend(&r.data);
        }
    }
    out
}

pub fn configurations() -> Vec<Config> {
    let mut out = Vec::new();
    for (fixture, bytes) in FILES.iter().enumerate() {
        let (_, ends) = raw(bytes);
        for count in [0, 1, 2, ends.len()] {
            let total = if count == 0 { 0 } else { ends[count - 1] };
            let start = if count < 2 { 0 } else { ends[count - 2] };
            let eos: std::collections::BTreeSet<_> = [
                0,
                1,
                start.saturating_sub(1),
                start,
                start + 1,
                total.saturating_sub(1),
                total,
                total + 1,
                total + 900,
            ]
            .into_iter()
            .collect();
            for group in [1, 2, usize::MAX] {
                for tail in [
                    Tail::Audio,
                    Tail::Nil,
                    Tail::Empty,
                    Tail::NonAudio,
                    Tail::Gap,
                ] {
                    for &end in &eos {
                        out.push(Config {
                            fixture,
                            group,
                            tail,
                            count,
                            end,
                        });
                    }
                }
            }
        }
    }
    out
}

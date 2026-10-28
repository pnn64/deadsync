//! Differential packet-origin lookups and public observations under packet policies.
use deadlib_audio_decode_ogg::*;
use std::io::{self, Write};

fn page(serial: u32, sequence: u32, flags: u8, granule: u64, laces: &[u8]) -> Vec<u8> {
    let length = laces.iter().map(|&n| n as usize).sum();
    let body: Vec<_> = (0..length)
        .map(|i| (i * 37 + serial as usize) as u8)
        .collect();
    encode_page(
        PageHeader {
            flags,
            granule_position: granule,
            serial,
            sequence,
        },
        laces,
        &body,
    )
    .unwrap()
}
fn fixtures() -> Vec<Vec<u8>> {
    let flat = [page(11, 0, BOS, 0, &[0]), page(11, 1, EOS, 1, &[0, 1, 254])].concat();
    let mut result = vec![flat];
    for (span, end) in [(UNKNOWN_GRANULE_POSITION, 2), (0, 0)] {
        result.push(
            [
                page(11, 0, BOS, 0, &[1]),
                page(22, 0, BOS, 0, &[1]),
                page(11, 1, 0, span, &[255]),
                page(11, 2, CONTINUED, UNKNOWN_GRANULE_POSITION, &[]),
                page(22, 1, 0, 1, &[0, 1, 254]),
                page(11, 3, CONTINUED, 1, &[0, 255]),
                page(22, 2, EOS, end, &[255, 0, 0]),
                page(11, 4, CONTINUED | EOS, end, &[0]),
            ]
            .concat(),
        );
    }
    result.push([page(11, 0, BOS, 0, &[1]), page(11, 1, EOS, 1, &[0, 255])].concat());
    result
}
fn main() {
    let mut out = io::BufWriter::new(io::stdout().lock());
    for (fixture, bytes) in fixtures().into_iter().enumerate() {
        write!(out, "INPUT{fixture}:{}:", bytes.len()).unwrap();
        out.write_all(&bytes).unwrap();
        let mut pages = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            let (page, length) = Page::parse(&bytes[offset..]).unwrap();
            pages.push((page, offset as u64));
            offset += length;
        }
        for packet in [0, 1, 254, 255, 256, 510, usize::MAX] {
            for pending in [0, 1, 254, 255, 256, 510, usize::MAX] {
                for flags in 0..4 {
                    let limits = Limits {
                        max_packet_bytes: packet,
                        max_pending_bytes: pending,
                        max_streams: 2,
                    };
                    let validation = ValidationOptions {
                        allow_zero_span_granules: flags & 1 != 0,
                        allow_decreasing_eos_granule: flags & 2 != 0,
                    };
                    write!(out, "POLICY{packet}/{pending}/{flags};").unwrap();
                    let mut demux = Demuxer::with_validation(limits, validation);
                    for &(page, offset) in &pages {
                        let result = demux.push_page(page, offset, |p| {
                            write!(out, "D{:?}N{}:", p.info, p.data.len()).unwrap();
                            out.write_all(p.data).unwrap();
                        });
                        write!(
                            out,
                            "R{result:?}A{}B{}C{}S{:?};",
                            demux.active_streams(),
                            demux.pending_bytes(),
                            demux.chain(),
                            demux.compatibility()
                        )
                        .unwrap();
                    }
                    write!(out, "F{:?}F{:?};", demux.finish(), demux.finish()).unwrap();
                    for serial in [11, 22] {
                        for start in [0, 2] {
                            let mut capture =
                                PacketCapture::with_validation(serial, limits, validation);
                            for &(page, offset) in pages.iter().skip(start) {
                                let result = capture.push_page(page, offset, |p| {
                                    write!(out, "C{:?}N{}:", p.info, p.data.len()).unwrap();
                                    out.write_all(p.data).unwrap();
                                });
                                write!(
                                    out,
                                    "R{result:?}B{}E{}S{:?};",
                                    capture.pending_bytes(),
                                    capture.is_ended(),
                                    capture.compatibility()
                                )
                                .unwrap();
                            }
                            write!(out, "F{:?}F{:?};", capture.finish(), capture.finish()).unwrap();
                        }
                    }
                    match OggIndex::read(
                        bytes.as_slice(),
                        IndexOptions {
                            container_limits: limits,
                            validation,
                            ..IndexOptions::default()
                        },
                    ) {
                        Ok(index) => {
                            write!(out, "INDEX{}S{:?};", index.length(), index.compatibility())
                                .unwrap();
                            for stream in index.streams() {
                                write!(out, "{stream:?}{:?};", index.pages(stream.serial)).unwrap();
                                for number in (0..=stream.packets).chain([u64::MAX - 1, u64::MAX]) {
                                    write!(
                                        out,
                                        "LOOKUP{}/{number}:{:?};",
                                        stream.serial,
                                        index.packet_start_page(stream.serial, number)
                                    )
                                    .unwrap();
                                }
                            }
                            write!(out, "MISSING{:?};", index.packet_start_page(u32::MAX, 0))
                                .unwrap();
                        }
                        Err(error) => write!(out, "INDEX{error:?};").unwrap(),
                    }
                }
            }
        }
    }
}

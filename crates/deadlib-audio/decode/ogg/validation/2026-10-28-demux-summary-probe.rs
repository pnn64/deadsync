use deadlib_audio_decode_ogg::*;
use std::io::{self, Cursor, Write};

fn fixture(streams: usize, target: usize, shuffled: bool) -> Vec<u8> {
    let mut logical = Vec::new();
    for stream in 0..streams {
        let serial = (stream as u32).wrapping_mul(0x9e37_79b9) ^ 0x1234_5678;
        let mut writer = PacketWriter::with_options(
            Vec::new(),
            serial,
            WriteOptions {
                target_page_body: target,
            },
        );
        writer.write_packet(b"h", 100, false).unwrap();
        for (i, size) in [0, 255, 65025 + stream % 3 * 255, 17]
            .into_iter()
            .enumerate()
        {
            let data: Vec<_> = (0..size)
                .map(|n| (n * 37 + stream * 11 + i) as u8)
                .collect();
            writer
                .write_packet(&data, (i + 11) as u64 * 10, i == 3 && stream % 2 == 0)
                .unwrap();
        }
        writer.finish().unwrap();
        let bytes = writer.into_inner();
        let mut pages = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            let (page, length) = Page::parse(&bytes[offset..]).unwrap();
            pages.push(page.bytes().to_vec());
            offset += length;
        }
        logical.push(pages);
    }
    let mut out = Vec::new();
    for round in 0..logical.iter().map(Vec::len).max().unwrap() {
        let mut order: Vec<_> = (0..streams).collect();
        if shuffled {
            let mut seed = round as u32 + 17;
            for i in (1..streams).rev() {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                order.swap(i, seed as usize % (i + 1));
            }
        }
        for stream in order {
            if let Some(page) = logical[stream].get(round) {
                out.extend(page);
            }
        }
    }
    out
}

fn main() {
    let mut out = io::BufWriter::new(io::stdout().lock());
    let mut cases = 0;
    for streams in [1, 2, 7, 65] {
        for target in [0, 8192] {
            for shuffled in [false, true] {
                let bytes = fixture(streams, target, shuffled);
                for max_pages in [0, 1, 5, usize::MAX] {
                    for stop in [1, 5, usize::MAX] {
                        write!(
                            out,
                            "CASE{cases}:{streams}/{target}/{shuffled}/{max_pages}/{stop};"
                        )
                        .unwrap();
                        cases += 1;
                        let mut reader = Cursor::new(bytes.as_slice());
                        let mut events = 0;
                        let mut expected_packets = Vec::new();
                        let result = OggIndex::read_with_events(
                            &mut reader,
                            IndexOptions {
                                max_pages,
                                ..IndexOptions::default()
                            },
                            |event| {
                                match event {
                                    IndexEvent::Packet { page, packet } => {
                                        expected_packets.push(packet.info);
                                        write!(
                                            out,
                                            "PACKET{:?}N{}C{}PAGE{:?};",
                                            packet.info,
                                            packet.data.len(),
                                            crc32(packet.data),
                                            page.header()
                                        )
                                        .unwrap();
                                    }
                                    IndexEvent::Page { page, offset } => {
                                        write!(
                                            out,
                                            "PAGE{offset}H{:?}N{}C{};",
                                            page.header(),
                                            page.bytes().len(),
                                            page.checksum()
                                        )
                                        .unwrap();
                                    }
                                }
                                events += 1;
                                if events == stop {
                                    Err(Error::Invalid("probe callback stop"))
                                } else {
                                    Ok(())
                                }
                            },
                        );
                        write!(out, "POSITION{};", reader.position()).unwrap();
                        match result {
                            Err(error) => write!(out, "ERROR{error:?};").unwrap(),
                            Ok(index) => {
                                write!(
                                    out,
                                    "INDEX{}B{}S{:?};",
                                    index.length(),
                                    index.storage_bytes(),
                                    index.compatibility()
                                )
                                .unwrap();
                                assert!(index.stream(0xdead_beef).is_none());
                                assert!(index.page_at_or_before(0xdead_beef, 0).is_none());
                                assert!(
                                    index.granule_at_or_before(0xdead_beef, u64::MAX).is_none()
                                );
                                assert!(index.packet_start_page(0xdead_beef, 0).is_none());
                                for stream in index.streams() {
                                    let pages = index.pages(stream.serial).unwrap();
                                    for offset in pages
                                        .iter()
                                        .flat_map(|page| {
                                            [
                                                page.offset.saturating_sub(1),
                                                page.offset,
                                                page.offset + page.length as u64 - 1,
                                                page.offset + page.length as u64,
                                            ]
                                        })
                                        .chain([0, u64::MAX])
                                    {
                                        let actual = index.page_at_or_before(stream.serial, offset);
                                        let expected =
                                            pages.iter().rev().find(|page| page.offset <= offset);
                                        assert_eq!(
                                            actual.map(|page| page.offset),
                                            expected.map(|page| page.offset)
                                        );
                                        write!(out, "OFFSET{offset}:{actual:?};").unwrap();
                                    }
                                    write!(
                                        out,
                                        "STREAM{stream:?}PAGES{:?};",
                                        index.pages(stream.serial)
                                    )
                                    .unwrap();
                                    for number in
                                        (0..=stream.packets).chain([u64::MAX - 1, u64::MAX])
                                    {
                                        let actual = index.packet_start_page(stream.serial, number);
                                        let expected = expected_packets.iter().find(|packet| {
                                            packet.serial == stream.serial
                                                && packet.number == number
                                        });
                                        assert_eq!(
                                            actual.map(|page| page.offset),
                                            expected.map(|packet| packet.start_page_offset)
                                        );
                                        write!(out, "PACKET{number}:{actual:?};").unwrap();
                                    }
                                    for granule in [
                                        0,
                                        1,
                                        7,
                                        18,
                                        40,
                                        99,
                                        100,
                                        101,
                                        110,
                                        120,
                                        130,
                                        140,
                                        u64::MAX,
                                    ] {
                                        let actual =
                                            index.granule_at_or_before(stream.serial, granule);
                                        let expected =
                                            index.pages(stream.serial).unwrap().iter().rev().find(
                                                |page| {
                                                    page.header.granule_position
                                                        != UNKNOWN_GRANULE_POSITION
                                                        && page.header.granule_position <= granule
                                                },
                                            );
                                        assert_eq!(
                                            actual.map(|page| page.offset),
                                            expected.map(|page| page.offset)
                                        );
                                        write!(out, "GRANULE{granule}:{actual:?};").unwrap();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out.flush().unwrap();
    eprintln!("DEMUX_SUMMARY_CASES {cases}");
}

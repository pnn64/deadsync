use deadlib_audio_decode_ogg::*;
use std::io::{self, Cursor, Read, Write};

fn page(
    flags: u8,
    granule: u64,
    serial: u32,
    sequence: u32,
    lacing: &[u8],
    data: &[u8],
) -> Vec<u8> {
    encode_page(
        PageHeader {
            flags,
            granule_position: granule,
            serial,
            sequence,
        },
        lacing,
        data,
    )
    .unwrap()
}
fn packets(bytes: &[u8]) -> Result<Vec<(Vec<u8>, PacketInfo)>, Error> {
    let mut reader = PageReader::new(Cursor::new(bytes));
    let mut demux = Demuxer::default();
    let mut out = Vec::new();
    while let Some(located) = reader.next_page()? {
        demux.push_page(located.page, located.offset, |p| {
            out.push((p.data.to_vec(), p.info))
        })?;
    }
    demux.finish()?;
    Ok(out)
}
fn fixture() -> (Vec<u8>, Vec<Vec<u8>>) {
    let sizes = [4, 0, 1, 254, 255, 256, 510, 65024, 65025, 130050, 0, 99];
    let data: Vec<Vec<u8>> = sizes
        .iter()
        .enumerate()
        .map(|(i, &len)| (0..len).map(|n| (n * 31 + i * 17) as u8).collect())
        .collect();
    let mut writer = PacketWriter::new(Vec::new(), 0x12345678);
    for (i, bytes) in data.iter().enumerate() {
        writer
            .write_packet(bytes, i as u64 * 100, i == data.len() - 1)
            .unwrap();
    }
    (writer.into_inner(), data)
}

#[test]
fn crc_matches_independent_bitwise_polynomial() {
    let data: Vec<u8> = (0..65536).map(|n| (n * 13 + n / 17) as u8).collect();
    let mut crc = 0u32;
    for &byte in &data {
        crc ^= (byte as u32) << 24;
        for _ in 0..8 {
            crc = (crc << 1)
                ^ if crc & 0x8000_0000 != 0 {
                    0x04c11db7
                } else {
                    0
                };
        }
    }
    assert_eq!(crc32(&data), crc);
    assert_eq!(crc32(b"123456789"), 0x89a1897f);
}

#[test]
fn native_writer_packets_and_metadata_match_reference_reader() {
    let (bytes, expected) = fixture();
    let native = packets(&bytes).unwrap();
    let mut reference = ogg::PacketReader::new(Cursor::new(&bytes));
    for (i, (data, info)) in native.iter().enumerate() {
        assert_eq!(data, &expected[i]);
        let other = reference.read_packet().unwrap().unwrap();
        assert_eq!(data, &other.data);
        assert_eq!(info.serial, other.stream_serial());
        assert_eq!(info.first_in_stream, other.first_in_stream());
        assert_eq!(info.last_in_stream, other.last_in_stream());
        if let Some(granule) = info.granule_position {
            assert_eq!(granule, other.absgp_page());
        }
    }
    assert_eq!(native.len(), expected.len());
    assert!(reference.read_packet().unwrap().is_none());
}

#[test]
fn reference_writer_packets_match_native_reader() {
    use ogg::writing::PacketWriteEndInfo::*;
    let (_, expected) = fixture();
    let mut writer = ogg::PacketWriter::new(Vec::new());
    for (i, data) in expected.iter().enumerate() {
        let end = if i == expected.len() - 1 {
            EndStream
        } else if i == 0 {
            EndPage
        } else {
            NormalPacket
        };
        writer
            .write_packet(data.as_slice(), 1234, end, i as u64 * 100)
            .unwrap();
    }
    let native = packets(&writer.into_inner()).unwrap();
    assert_eq!(
        native.iter().map(|p| &p.0).collect::<Vec<_>>(),
        expected.iter().collect::<Vec<_>>()
    );
}

#[test]
fn continued_zero_terminator_and_granule_on_last_complete_packet() {
    let mut bytes = page(BOS, 0, 1, 0, &[1], b"h");
    // A complete packet, then an unfinished 255-byte packet on one page.
    let mut body = b"a".to_vec();
    body.extend_from_slice(&[42; 255]);
    let offset = bytes.len() as u64;
    bytes.extend(page(0, 7, 1, 1, &[1, 255], &body));
    bytes.extend(page(CONTINUED | EOS, 9, 1, 2, &[0, 0], &[]));
    let decoded = packets(&bytes).unwrap();
    assert_eq!(decoded.len(), 4);
    assert_eq!(decoded[1].1.granule_position, Some(7));
    assert_eq!(decoded[2].0, vec![42; 255]);
    assert_eq!(decoded[2].1.granule_position, None);
    assert_eq!(decoded[2].1.start_page_offset, offset);
    assert!(decoded[3].0.is_empty());
    assert_eq!(decoded[3].1.granule_position, Some(9));
    assert!(decoded[3].1.last_in_stream);
}

#[test]
fn multiplexed_groups_and_chains_and_nil_eos() {
    let mut bytes = page(BOS, 0, 1, 0, &[1], b"a");
    bytes.extend(page(BOS, 0, 2, 0, &[1], b"b"));
    bytes.extend(page(0, 5, 2, 1, &[1], b"c"));
    bytes.extend(page(EOS, 8, 1, 1, &[1], b"d"));
    bytes.extend(page(EOS, 5, 2, 2, &[], &[]));
    bytes.extend(page(BOS | EOS, 0, 3, 0, &[0], &[]));
    let decoded = packets(&bytes).unwrap();
    assert_eq!(
        decoded.iter().map(|p| p.1.serial).collect::<Vec<_>>(),
        [1, 2, 2, 1, 3]
    );
    assert_eq!(
        decoded.iter().map(|p| p.1.chain).collect::<Vec<_>>(),
        [0, 0, 0, 0, 1]
    );
}

struct Fragmented<R> {
    inner: R,
    max: usize,
    interrupted: bool,
}
impl<R: Read> Read for Fragmented<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.interrupted = !self.interrupted;
        if self.interrupted {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let len = out.len().min(self.max);
        self.inner.read(&mut out[..len])
    }
}
impl<W: Write> Write for Fragmented<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.interrupted = !self.interrupted;
        if self.interrupted {
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.inner.write(&data[..data.len().min(self.max)])
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[test]
fn partial_interrupted_reads_and_writes() {
    let (bytes, expected) = fixture();
    let mut reader = PageReader::new(Fragmented {
        inner: Cursor::new(&bytes),
        max: 7,
        interrupted: false,
    });
    let mut demux = Demuxer::default();
    let mut output = Vec::new();
    while let Some(located) = reader.next_page().unwrap() {
        demux
            .push_page(located.page, located.offset, |p| {
                output.push(p.data.to_vec())
            })
            .unwrap();
    }
    demux.finish().unwrap();
    assert_eq!(output, expected);
    let mut writer = PacketWriter::new(
        Fragmented {
            inner: Vec::new(),
            max: 3,
            interrupted: false,
        },
        0x12345678,
    );
    for (i, data) in expected.iter().enumerate() {
        writer
            .write_packet(data, i as u64 * 100, i == expected.len() - 1)
            .unwrap();
    }
    assert_eq!(writer.into_inner().inner, bytes);
}

#[test]
fn incremental_every_header_split_and_small_body_chunks() {
    let (bytes, expected) = fixture();
    for chunk in [1, 2, 3, 26, 27, 28, 254, 255, 256, 282, 4096, bytes.len()] {
        let mut decoder = PageDecoder::new();
        let mut demux = Demuxer::default();
        let mut output = Vec::new();
        let mut offset = 0;
        for input in bytes.chunks(chunk) {
            let mut position = 0;
            while position < input.len() {
                let progress = decoder.push(&input[position..]).unwrap();
                assert!(progress.consumed != 0);
                position += progress.consumed;
                if let Some(page) = progress.page {
                    demux
                        .push_page(page, offset, |p| output.push(p.data.to_vec()))
                        .unwrap();
                    offset += page.bytes().len() as u64;
                }
            }
        }
        decoder.finish().unwrap();
        demux.finish().unwrap();
        assert_eq!(output, expected, "chunk {chunk}");
    }
}

#[test]
fn recovery_finds_capture_inside_false_page_and_after_crc_error() {
    let valid = page(BOS | EOS, 0, 8, 0, &[5], b"hello");
    let mut false_page = page(BOS | EOS, 0, 7, 0, &[100], &[99; 100]);
    false_page[32..32 + valid.len()].copy_from_slice(&valid);
    let mut bytes = b"noiseOgg".to_vec();
    bytes.extend(false_page);
    let mut reader = PageReader::with_mode(Cursor::new(&bytes), SyncMode::Recover);
    let found = reader.next_page().unwrap().unwrap();
    assert_eq!(found.offset, 40);
    assert_eq!(found.page.body(), b"hello");
    assert!(reader.next_page().unwrap().is_none());
    assert_eq!(reader.discarded_bytes(), (bytes.len() - valid.len()) as u64);
    // Also recover if a false capture promises a page beyond EOF.
    let mut bytes = page(0, UNKNOWN_GRANULE_POSITION, 9, 0, &[255], &[1; 255]);
    bytes.truncate(29);
    bytes.extend(&valid);
    let mut reader = PageReader::with_mode(Cursor::new(bytes), SyncMode::Recover);
    assert_eq!(reader.next_page().unwrap().unwrap().page.body(), b"hello");
}

#[test]
fn recovery_scans_batches_and_preserves_split_captures_and_read_latency() {
    struct Chunked<'a> {
        bytes: &'a [u8],
        chunk: usize,
        calls: usize,
        error_at_end: bool,
    }
    impl Read for Chunked<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            self.calls += 1;
            if self.bytes.is_empty() && self.error_at_end {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let count = out.len().min(self.chunk).min(self.bytes.len());
            out[..count].copy_from_slice(&self.bytes[..count]);
            self.bytes = &self.bytes[count..];
            Ok(count)
        }
    }
    let valid = page(BOS | EOS, 0, 8, 0, &[5], b"hello");
    for chunk in [1, 2, 3, 7, 4093, 4094, 4095, 4096, 4097] {
        let mut bytes = vec![b'x'; 8191];
        bytes.extend(&valid);
        bytes.extend(b"junk");
        bytes.extend(&valid);
        bytes.extend(b"Ogg"); // An incomplete capture at EOF is discarded.
        let source = Chunked {
            bytes: &bytes,
            chunk,
            calls: 0,
            error_at_end: false,
        };
        let mut reader = PageReader::with_mode(source, SyncMode::Recover);
        let found = reader.next_page().unwrap().unwrap();
        assert_eq!(found.offset, 8191);
        assert_eq!(found.page.body(), b"hello");
        let found = reader.next_page().unwrap().unwrap();
        assert_eq!(found.offset, 8191 + valid.len() as u64 + 4);
        assert_eq!(found.page.body(), b"hello");
        assert!(reader.next_page().unwrap().is_none());
        assert_eq!(reader.discarded_bytes(), 8198);
        if chunk >= 4093 {
            assert!(reader.into_inner().calls < 15);
        }
    }
    let mut bytes = b"noise".to_vec();
    bytes.extend(&valid);
    let source = Chunked {
        bytes: &bytes,
        chunk: usize::MAX,
        calls: 0,
        error_at_end: true,
    };
    let mut reader = PageReader::with_mode(source, SyncMode::Recover);
    assert_eq!(reader.next_page().unwrap().unwrap().page.body(), b"hello");
    // Once a complete page is available, recovery must return it before
    // requesting further input (which may block or fail on a live stream).
    assert!(matches!(reader.next_page(), Err(Error::Io(_))));
}

#[test]
fn page_and_physical_validation_reject_malformed_data() {
    let valid = page(BOS | EOS, 0, 1, 0, &[3], b"abc");
    for end in 0..valid.len() {
        assert!(Page::parse(&valid[..end]).is_err());
    }
    for position in 0..valid.len() {
        let mut bad = valid.clone();
        bad[position] ^= 128;
        assert!(Page::parse(&bad).is_err(), "byte {position}");
    }
    let bos = page(BOS, 0, 1, 0, &[1], b"h");
    let cases = [
        page(0, 1, 2, 0, &[1], b"x"),                                 // no BOS
        page(BOS | EOS, 1, 1, 0, &[1], b"x"),                         // duplicate serial
        page(EOS, 1, 1, 2, &[1], b"x"),                               // sequence hole
        page(CONTINUED | EOS, 1, 1, 1, &[1], b"x"),                   // no partial
        page(EOS, UNKNOWN_GRANULE_POSITION, 1, 1, &[255], &[0; 255]), // incomplete EOS
        page(0, 1, 1, 1, &[255], &[0; 255]),                          // granule without completion
    ];
    for bad in cases {
        let mut bytes = bos.clone();
        bytes.extend(bad);
        assert!(packets(&bytes).is_err());
    }
    let mut demux = Demuxer::default();
    assert!(demux.finish().is_err());
    assert_eq!(
        demux.push_page(Page::parse(&valid).unwrap().0, 0, |_| {}),
        Err(Error::Failed)
    );
    demux.reset();
    demux
        .push_page(Page::parse(&valid).unwrap().0, 0, |_| {})
        .unwrap();
    demux.finish().unwrap();
}

#[test]
fn unfinished_input_limits_and_terminal_error_states() {
    let (bytes, _) = fixture();
    let mut demux = Demuxer::new(Limits {
        max_packet_bytes: 300,
        ..Limits::default()
    });
    let mut reader = PageReader::new(Cursor::new(&bytes));
    let mut failed = false;
    while let Some(located) = reader.next_page().unwrap() {
        if let Err(error) = demux.push_page(located.page, located.offset, |_| {}) {
            assert_eq!(error, Error::LimitExceeded("packet size"));
            failed = true;
            break;
        }
    }
    assert!(failed);
    assert_eq!(demux.finish(), Err(Error::Failed));
    let mut decoder = PageDecoder::new();
    decoder.push(&bytes[..15]).unwrap();
    assert_eq!(decoder.finish(), Err(Error::Truncated));
    assert!(matches!(decoder.push(&[]), Err(Error::Failed)));
    decoder.reset();
    assert!(decoder.push(&bytes).unwrap().page.is_some());
    let mut reader = PageReader::new(Cursor::new(&bytes[..15]));
    assert!(matches!(reader.next_page(), Err(Error::Truncated)));
    assert!(matches!(reader.next_page(), Err(Error::Failed)));
}

#[test]
fn borrowed_packets_do_not_copy_and_finish_emits_nil_eos() {
    let mut writer = PacketWriter::new(Vec::new(), 1);
    writer.write_packet(b"header", 0, false).unwrap();
    writer.finish().unwrap();
    writer.finish().unwrap();
    let bytes = writer.into_inner();
    let (first, len) = Page::parse(&bytes).unwrap();
    let mut demux = Demuxer::default();
    demux
        .push_page(first, 0, |p| {
            assert_eq!(p.data.as_ptr(), first.body().as_ptr())
        })
        .unwrap();
    let (last, _) = Page::parse(&bytes[len..]).unwrap();
    assert!(last.header().is_eos());
    assert!(last.lacing().is_empty());
    demux
        .push_page(last, len as u64, |_| panic!("nil page has no packet"))
        .unwrap();
    demux.finish().unwrap();
}

#[test]
fn deterministic_random_packets_differential() {
    let mut seed = 0x9428_7381u32;
    let mut writer = PacketWriter::with_options(
        Vec::new(),
        817,
        WriteOptions {
            target_page_body: 379,
        },
    );
    let mut expected = Vec::new();
    for i in 0..500 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let len = if i == 0 { 2 } else { seed as usize % 100000 };
        let data: Vec<u8> = (0..len).map(|n| (n as u32 ^ seed) as u8).collect();
        writer.write_packet(&data, i, i == 499).unwrap();
        expected.push(data);
    }
    let bytes = writer.into_inner();
    let native = packets(&bytes).unwrap();
    let mut reference = ogg::PacketReader::new(Cursor::new(bytes));
    for ((data, _), expected) in native.iter().zip(&expected) {
        assert_eq!(data, expected);
        assert_eq!(*data, reference.read_packet().unwrap().unwrap().data);
    }
    assert_eq!(native.len(), 500);
    assert!(reference.read_packet().unwrap().is_none());
}

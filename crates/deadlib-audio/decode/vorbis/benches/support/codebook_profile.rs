// Count actual codebook use in an instrumented copy; never time these counters.
use deadlib_audio_decode_vorbis as real;

fn check_counters() {
    use profiled::{
        Error, Limits,
        bitpack::{BitReader, BitWriter},
        codebook::{Codebook, Huffman},
    };
    let mut lengths: Vec<_> = (1..=32).collect();
    lengths.push(32);
    let tree = Huffman::from_lengths(&lengths, 4096).unwrap();
    assert_eq!(tree.decode(&mut BitReader::new(&[255; 4])), Ok(32));
    assert_eq!(tree.decode(&mut BitReader::new(&[0])), Ok(0));
    assert_eq!(
        tree.decode(&mut BitReader::new(&[])),
        Err(Error::EndOfPacket)
    );
    assert_eq!(
        tree.profile_counts(),
        [3, 0, 1, 1, 1, 0, 32 - tree.profile_width() as u64, 0]
    );
    let tree = Huffman::from_lengths(&[0, 1], 0).unwrap();
    for value in [0, 255] {
        assert_eq!(tree.decode(&mut BitReader::new(&[value])), Ok(1));
    }
    assert_eq!(
        tree.decode(&mut BitReader::new(&[])),
        Err(Error::EndOfPacket)
    );
    assert_eq!(tree.profile_counts(), [3, 3, 0, 0, 0, 0, 0, 0]);
    let mut wire = BitWriter::new();
    for (value, width) in [
        (0x564342, 24),
        (1, 16),
        (2, 24),
        (0, 1),
        (0, 1),
        (0, 5),
        (0, 5),
        (2, 4),
        (0, 32),
        ((788 << 21) | 1, 32),
        (1, 4),
        (0, 1),
        (1, 2),
        (2, 2),
    ] {
        wire.write(value, width).unwrap();
    }
    let wire = wire.finish().unwrap();
    let book = Codebook::read(&mut BitReader::new(&wire), &Limits::default()).unwrap();
    assert_eq!(
        book.decode_vector(&mut BitReader::new(&[])),
        Err(Error::EndOfPacket)
    );
    assert_eq!(
        book.decode_vector(&mut BitReader::new(&[1])).unwrap(),
        [2.0]
    );
    assert_eq!(book.decode_scalar(&mut BitReader::new(&[0])), Ok(0));
    let stats = book.profile();
    assert_eq!(
        (stats.scalar_calls, stats.vector_calls, stats.errors),
        (3, 2, [1, 0])
    );
    assert_eq!(stats.entries, [1, 1]);
    assert_eq!(stats.lengths[1], 2);
    assert_eq!(book.huffman().profile_counts(), [3, 0, 0, 0, 0, 3, 2, 0]);
}

fn main() {
    check_counters();
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2);
    let bytes = std::fs::read(&args[0]).unwrap();
    let mut pages = deadlib_audio_decode_ogg::PageReader::new(bytes.as_slice());
    let mut demux = deadlib_audio_decode_ogg::Demuxer::with_validation(
        Default::default(),
        deadlib_audio_decode_ogg::ValidationOptions {
            allow_zero_span_granules: args[1] == "1",
            ..Default::default()
        },
    );
    let mut packets = Vec::new();
    let mut serial = None;
    while let Some(page) = pages.next_page().unwrap() {
        demux
            .push_page(page.page, page.offset, |packet| {
                assert_eq!(
                    *serial.get_or_insert(packet.info.serial),
                    packet.info.serial
                );
                packets.push(packet.data.to_vec());
            })
            .unwrap();
    }
    demux.finish().unwrap();
    let id = real::Identification::parse(&packets[0]).unwrap();
    let setup = real::Setup::parse(&packets[2], &id, &real::Limits::default()).unwrap();
    let mut native = real::PacketDecoder::new(setup).unwrap();
    let id = profiled::Identification::parse(&packets[0]).unwrap();
    let setup = profiled::Setup::parse(&packets[2], &id, &profiled::Limits::default()).unwrap();
    let mut copy = profiled::PacketDecoder::new(setup).unwrap();
    // Ample setup budget is required so diagnostic struct fields cannot alter
    // the optional prefix widths. The frequency arrays themselves are unbudgeted.
    assert!(native.setup().storage_bytes() < real::Limits::default().max_setup_memory / 2);
    assert!(copy.setup().storage_bytes() < profiled::Limits::default().max_setup_memory / 2);
    let mut samples = 0u64;
    let mut truncated = 0;
    let mut discarded = 0;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for packet in &packets[3..] {
        let a = native.decode(packet).unwrap();
        let b = copy.decode(packet).unwrap();
        assert_eq!(a.is_some(), b.is_some());
        if let (Some(a), Some(b)) = (a, b) {
            assert_eq!(
                (
                    a.frames,
                    a.channels,
                    a.start_frame,
                    a.block_size,
                    a.truncated
                ),
                (
                    b.frames,
                    b.channels,
                    b.start_frame,
                    b.block_size,
                    b.truncated
                )
            );
            truncated += u64::from(a.truncated);
            samples += (a.frames * a.channels) as u64;
            for (a, b) in a.planes().zip(b.planes()) {
                for (&a, &b) in a.iter().zip(b) {
                    assert!(a.is_finite());
                    assert_eq!(a.to_bits(), b.to_bits(), "instrumented PCM differs");
                    hash = (hash ^ a.to_bits() as u64).wrapping_mul(0x100_0000_01b3);
                }
            }
        } else {
            discarded += 1;
        }
    }
    let books = copy.setup().codebooks();
    assert_eq!(books.len(), native.setup().codebooks().len());
    let mut roles = vec![0u8; books.len()];
    for floor in copy.setup().floors() {
        match floor {
            profiled::Floor::Type0(floor) => {
                for &book in &floor.books {
                    roles[book as usize] |= 2;
                }
            }
            profiled::Floor::Type1(floor) => {
                for class in &floor.classes {
                    if let Some(book) = class.masterbook {
                        roles[book as usize] |= 1;
                    }
                    for book in class.books.iter().flatten() {
                        roles[*book as usize] |= 1;
                    }
                }
            }
        }
    }
    for residue in copy.setup().residues() {
        roles[residue.classbook as usize] |= 4;
        for row in &residue.books {
            for book in row.iter().flatten() {
                roles[*book as usize] |= 8;
            }
        }
    }
    println!(
        "{{\"bit_exact\":true,\"instrumentation_self_tests_passed\":true,\"raw_channel_samples\":{samples},\"audio_packets\":{},\"discarded_packets\":{discarded},\"truncated_packets\":{truncated},\"zero_span_granules\":{},\"books\":{},\"pcm_bits_fnv1a64\":\"{hash:016x}\"}}",
        packets.len() - 3,
        demux.compatibility().zero_span_granules,
        books.len()
    );
    for (index, book) in books.iter().enumerate() {
        assert_eq!(book.lengths(), native.setup().codebooks()[index].lengths());
        let active = book.lengths().iter().filter(|&&length| length != 0).count();
        let maximum = book.lengths().iter().copied().max().unwrap();
        let width = if active == 1 || maximum <= 2 {
            0
        } else {
            maximum.min(8).min(real::bitpack::ilog(active as u32) as u8)
        };
        assert_eq!(book.huffman().profile_width(), width);
        let profile = book.profile();
        println!(
            "{{\"book\":{index},\"roles\":{},\"dimensions\":{},\"entries\":{},\"active_entries\":{active},\"lookup_type\":{},\"prefix_bits\":{width},\"scalar_calls\":{},\"vector_calls\":{},\"errors\":{:?},\"length_counts\":{:?},\"entry_counts\":{:?},\"codeword_lengths\":{:?},\"huffman\":{:?}}}",
            roles[index],
            book.dimensions(),
            book.entries(),
            book.lookup_type(),
            profile.scalar_calls,
            profile.vector_calls,
            profile.errors,
            profile.lengths,
            profile.entries,
            book.lengths(),
            book.huffman().profile_counts()
        );
    }
}

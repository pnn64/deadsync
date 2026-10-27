use deadlib_audio_decode_flac::{
    Error, Limits,
    bitstream::{BitReader, BitWriter},
    checksum::{Md5, crc8, crc16},
    metadata::*,
};

#[test]
fn packed_fields_at_every_bit_offset() {
    for offset in 0..8 {
        let mut w = BitWriter::new();
        w.write(0, offset).unwrap();
        for width in 0..=56 {
            w.write(0x123456789abcdef, width).unwrap();
        }
        for width in 1..=56 {
            w.signed(-17, width).unwrap();
        }
        for count in [0, 1, 7, 8, 63, 64, 65, 1024] {
            w.unary(count).unwrap();
        }
        let mut r = BitReader::new(w.as_bytes());
        assert_eq!(r.read(offset).unwrap(), 0);
        for width in 0..=56 {
            assert_eq!(
                r.read(width).unwrap(),
                0x123456789abcdef & ((1u64 << width) - 1)
            );
        }
        for width in 1..=56 {
            assert_eq!(
                r.signed(width).unwrap(),
                (-17i64 << (64 - width)) >> (64 - width)
            );
        }
        for count in [0, 1, 7, 8, 63, 64, 65, 1024] {
            assert_eq!(r.unary(1024).unwrap(), count);
        }
        r.align_zero().unwrap();
        assert_eq!(r.read(1), Err(Error::Truncated));
    }
}

#[test]
fn checksum_known_answers_and_chunking() {
    assert_eq!(crc8(b"123456789"), 0xf4);
    assert_eq!(crc16(b"123456789"), 0xfee8);
    for (data, expected) in [
        ("", "d41d8cd98f00b204e9800998ecf8427e"),
        ("abc", "900150983cd24fb0d6963f7d28e17f72"),
        ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
        (
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
            "d174ab98d277d9f5a5611c2c9f419d9f",
        ),
        (
            "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
            "57edf4a22be3c955ac49da2e2107b67a",
        ),
    ] {
        for chunk in 1..=80 {
            let mut md5 = Md5::new();
            for bytes in data.as_bytes().chunks(chunk) {
                md5.update(bytes);
            }
            let actual: String = md5.finalize().iter().map(|x| format!("{x:02x}")).collect();
            assert_eq!(actual, expected);
        }
    }
}

fn info() -> StreamInfo {
    StreamInfo {
        min_block_size: 16,
        max_block_size: 65535,
        min_frame_size: 0,
        max_frame_size: 0,
        sample_rate: 1048575,
        channels: 8,
        bits_per_sample: 32,
        total_samples: (1 << 36) - 1,
        md5: [0xab; 16],
    }
}
#[test]
fn metadata_precision_preservation_and_limits() {
    let streaminfo = info().encode().unwrap();
    assert_eq!(StreamInfo::parse(&streaminfo).unwrap(), info());
    let mut data = b"fLaC".to_vec();
    MetadataBlock {
        kind: 0,
        data: &streaminfo,
    }
    .write(false, &mut data)
    .unwrap();
    MetadataBlock {
        kind: 126,
        data: b"opaque\0\xff",
    }
    .write(true, &mut data)
    .unwrap();
    let metadata = Metadata::parse(&data, Limits::default()).unwrap();
    assert_eq!(metadata.audio_offset, data.len());
    assert_eq!(metadata.encode().unwrap(), data);
    for end in 0..data.len() {
        assert!(Metadata::parse(&data[..end], Limits::default()).is_err());
    }
    assert!(matches!(
        Metadata::parse(
            &data,
            Limits {
                metadata_bytes: 33,
                ..Limits::default()
            }
        ),
        Err(Error::LimitExceeded(_))
    ));
    assert!(matches!(
        Metadata::parse(
            &data,
            Limits {
                metadata_blocks: 1,
                ..Limits::default()
            }
        ),
        Err(Error::LimitExceeded(_))
    ));
}

#[test]
fn comments_preserve_duplicates_and_binary_fields() {
    let mut data = 4u32.to_le_bytes().to_vec();
    data.extend_from_slice(b"test");
    data.extend_from_slice(&4u32.to_le_bytes());
    for field in [
        b"ARTIST=one".as_slice(),
        b"artist=two",
        b"WAVEFORMATEXTENSIBLE_CHANNEL_MASK=0X80000001",
        b"\xff",
    ] {
        data.extend_from_slice(&(field.len() as u32).to_le_bytes());
        data.extend_from_slice(field);
    }
    let comments = Comments::parse(&data).unwrap();
    assert_eq!(
        comments.values("Artist").collect::<Vec<_>>(),
        [b"one", b"two"]
    );
    assert_eq!(comments.channel_mask(), Some(0x80000001));
    assert_eq!(comments.fields().last(), Some(b"\xff".as_slice()));
    for end in 0..data.len() {
        assert!(Comments::parse(&data[..end]).is_err());
    }
}

#[test]
fn picture_cuesheet_and_seektable_boundaries() {
    let mut picture = vec![0; 32];
    assert!(Picture::parse(&picture).unwrap().data.is_empty());
    picture[28..32].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(Picture::parse(&picture).unwrap_err(), Error::Truncated);
    let mut cue = vec![0; 396 + 36];
    cue[395] = 1;
    cue[404] = 255;
    let sheet = CueSheet::parse(&cue).unwrap();
    assert_eq!(sheet.tracks[0].number, 255);
    for end in 0..cue.len() {
        assert!(CueSheet::parse(&cue[..end]).is_err());
    }
    let mut seeks = vec![0; 54];
    seeks[18..26].copy_from_slice(&100u64.to_be_bytes());
    seeks[36..44].copy_from_slice(&u64::MAX.to_be_bytes());
    assert_eq!(SeekTable::parse(&seeks).unwrap().points().count(), 3);
    seeks[18..26].copy_from_slice(&0u64.to_be_bytes());
    assert!(SeekTable::parse(&seeks).is_err());
}

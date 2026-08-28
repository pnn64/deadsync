use super::{SINGLE_BYTE_MAX, U128_BYTE, U16_BYTE, U32_BYTE, U64_BYTE};
use crate::{config::Endianness, enc::write::Writer, error::EncodeError};

#[inline]
fn write_u16<W: Writer>(
    writer: &mut W,
    marker: u8,
    endian: Endianness,
    value: u16,
) -> Result<(), EncodeError> {
    let payload = match endian {
        Endianness::Big => value.to_be_bytes(),
        Endianness::Little => value.to_le_bytes(),
    };
    writer.write(&[marker, payload[0], payload[1]])
}

#[inline]
fn write_u32<W: Writer>(
    writer: &mut W,
    marker: u8,
    endian: Endianness,
    value: u32,
) -> Result<(), EncodeError> {
    let payload = match endian {
        Endianness::Big => value.to_be_bytes(),
        Endianness::Little => value.to_le_bytes(),
    };
    writer.write(&[marker, payload[0], payload[1], payload[2], payload[3]])
}

#[inline]
fn write_u64<W: Writer>(
    writer: &mut W,
    marker: u8,
    endian: Endianness,
    value: u64,
) -> Result<(), EncodeError> {
    let payload = match endian {
        Endianness::Big => value.to_be_bytes(),
        Endianness::Little => value.to_le_bytes(),
    };
    writer.write(&[
        marker, payload[0], payload[1], payload[2], payload[3], payload[4], payload[5], payload[6],
        payload[7],
    ])
}

#[inline]
fn write_u128<W: Writer>(
    writer: &mut W,
    marker: u8,
    endian: Endianness,
    value: u128,
) -> Result<(), EncodeError> {
    let payload = match endian {
        Endianness::Big => value.to_be_bytes(),
        Endianness::Little => value.to_le_bytes(),
    };
    let mut encoded = [0u8; 17];
    encoded[0] = marker;
    encoded[1..].copy_from_slice(&payload);
    writer.write(&encoded)
}

pub fn varint_encode_u16<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: u16,
) -> Result<(), EncodeError> {
    if val <= SINGLE_BYTE_MAX.into() {
        writer.write(&[val as u8])
    } else {
        write_u16(writer, U16_BYTE, endian, val)
    }
}

pub fn varint_encode_u32<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: u32,
) -> Result<(), EncodeError> {
    if val <= SINGLE_BYTE_MAX.into() {
        writer.write(&[val as u8])
    } else if val <= u16::MAX.into() {
        write_u16(writer, U16_BYTE, endian, val as u16)
    } else {
        write_u32(writer, U32_BYTE, endian, val)
    }
}

pub fn varint_encode_u64<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: u64,
) -> Result<(), EncodeError> {
    if val <= SINGLE_BYTE_MAX.into() {
        writer.write(&[val as u8])
    } else if val <= u16::MAX.into() {
        write_u16(writer, U16_BYTE, endian, val as u16)
    } else if val <= u32::MAX.into() {
        write_u32(writer, U32_BYTE, endian, val as u32)
    } else {
        write_u64(writer, U64_BYTE, endian, val)
    }
}

pub fn varint_encode_u128<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: u128,
) -> Result<(), EncodeError> {
    if val <= SINGLE_BYTE_MAX.into() {
        writer.write(&[val as u8])
    } else if val <= u16::MAX.into() {
        write_u16(writer, U16_BYTE, endian, val as u16)
    } else if val <= u32::MAX.into() {
        write_u32(writer, U32_BYTE, endian, val as u32)
    } else if val <= u64::MAX.into() {
        write_u64(writer, U64_BYTE, endian, val as u64)
    } else {
        write_u128(writer, U128_BYTE, endian, val)
    }
}

pub fn varint_encode_usize<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: usize,
) -> Result<(), EncodeError> {
    // usize is being encoded as a u64
    varint_encode_u64(writer, endian, val as u64)
}

#[test]
fn test_encode_u16() {
    use crate::enc::write::SliceWriter;
    let mut buffer = [0u8; 20];

    // these should all encode to a single byte
    for i in 0u16..=u16::from(SINGLE_BYTE_MAX) {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u16(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(u16::from(buffer[0]), i);

        // Assert endianness doesn't matter
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u16(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(u16::from(buffer[0]), i);
    }

    // these values should encode in 3 bytes (leading byte + 2 bytes)
    // Values chosen at random, add new cases as needed
    for i in [
        u16::from(SINGLE_BYTE_MAX) + 1,
        300,
        500,
        700,
        888,
        1234,
        u16::MAX,
    ] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u16(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &i.to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u16(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &i.to_le_bytes());
    }
}

#[test]
fn test_encode_u32() {
    use crate::enc::write::SliceWriter;
    let mut buffer = [0u8; 20];

    // these should all encode to a single byte
    for i in 0u32..=u32::from(SINGLE_BYTE_MAX) {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u32(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(u32::from(buffer[0]), i);

        // Assert endianness doesn't matter
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u32(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(u32::from(buffer[0]), i);
    }

    // these values should encode in 3 bytes (leading byte + 2 bytes)
    // Values chosen at random, add new cases as needed
    for i in [
        u32::from(SINGLE_BYTE_MAX) + 1,
        300,
        500,
        700,
        888,
        1234,
        u32::from(u16::MAX),
    ] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u32(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &(i as u16).to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u32(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &(i as u16).to_le_bytes());
    }

    // these values should encode in 5 bytes (leading byte + 4 bytes)
    // Values chosen at random, add new cases as needed
    for i in [u32::from(u16::MAX) + 1, 100_000, 1_000_000, u32::MAX] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u32(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(buffer[0], U32_BYTE);
        assert_eq!(&buffer[1..5], &i.to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u32(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(buffer[0], U32_BYTE);
        assert_eq!(&buffer[1..5], &i.to_le_bytes());
    }
}

#[test]
fn test_encode_u64() {
    use crate::enc::write::SliceWriter;
    let mut buffer = [0u8; 20];

    // these should all encode to a single byte
    for i in 0u64..=u64::from(SINGLE_BYTE_MAX) {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(u64::from(buffer[0]), i);

        // Assert endianness doesn't matter
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(u64::from(buffer[0]), i);
    }

    // these values should encode in 3 bytes (leading byte + 2 bytes)
    // Values chosen at random, add new cases as needed
    for i in [
        u64::from(SINGLE_BYTE_MAX) + 1,
        300,
        500,
        700,
        888,
        1234,
        u64::from(u16::MAX),
    ] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &(i as u16).to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &(i as u16).to_le_bytes());
    }

    // these values should encode in 5 bytes (leading byte + 4 bytes)
    // Values chosen at random, add new cases as needed
    for i in [
        u64::from(u16::MAX) + 1,
        100_000,
        1_000_000,
        u64::from(u32::MAX),
    ] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(buffer[0], U32_BYTE);
        assert_eq!(&buffer[1..5], &(i as u32).to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(buffer[0], U32_BYTE);
        assert_eq!(&buffer[1..5], &(i as u32).to_le_bytes());
    }

    // these values should encode in 9 bytes (leading byte + 8 bytes)
    // Values chosen at random, add new cases as needed
    for i in [u64::from(u32::MAX) + 1, 5_000_000_000, u64::MAX] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 9);
        assert_eq!(buffer[0], U64_BYTE);
        assert_eq!(&buffer[1..9], &i.to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 9);
        assert_eq!(buffer[0], U64_BYTE);
        assert_eq!(&buffer[1..9], &i.to_le_bytes());
    }
}

#[test]
fn test_encode_u128() {
    use crate::enc::write::SliceWriter;
    let mut buffer = [0u8; 20];

    // these should all encode to a single byte
    for i in 0u128..=u128::from(SINGLE_BYTE_MAX) {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(u128::from(buffer[0]), i);

        // Assert endianness doesn't matter
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(u128::from(buffer[0]), i);
    }

    // these values should encode in 3 bytes (leading byte + 2 bytes)
    // Values chosen at random, add new cases as needed
    for i in [
        u128::from(SINGLE_BYTE_MAX) + 1,
        300,
        500,
        700,
        888,
        1234,
        u128::from(u16::MAX),
    ] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &(i as u16).to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &(i as u16).to_le_bytes());
    }

    // these values should encode in 5 bytes (leading byte + 4 bytes)
    // Values chosen at random, add new cases as needed
    for i in [
        u128::from(u16::MAX) + 1,
        100_000,
        1_000_000,
        u128::from(u32::MAX),
    ] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(buffer[0], U32_BYTE);
        assert_eq!(&buffer[1..5], &(i as u32).to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(buffer[0], U32_BYTE);
        assert_eq!(&buffer[1..5], &(i as u32).to_le_bytes());
    }

    // these values should encode in 9 bytes (leading byte + 8 bytes)
    // Values chosen at random, add new cases as needed
    for i in [
        u128::from(u32::MAX) + 1,
        5_000_000_000,
        u128::from(u64::MAX),
    ] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 9);
        assert_eq!(buffer[0], U64_BYTE);
        assert_eq!(&buffer[1..9], &(i as u64).to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 9);
        assert_eq!(buffer[0], U64_BYTE);
        assert_eq!(&buffer[1..9], &(i as u64).to_le_bytes());
    }

    // these values should encode in 17 bytes (leading byte + 16 bytes)
    // Values chosen at random, add new cases as needed
    for i in [u128::from(u64::MAX) + 1, u128::MAX] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 17);
        assert_eq!(buffer[0], U128_BYTE);
        assert_eq!(&buffer[1..17], &i.to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 17);
        assert_eq!(buffer[0], U128_BYTE);
        assert_eq!(&buffer[1..17], &i.to_le_bytes());
    }
}

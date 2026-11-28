use bincode::{
    config::Config,
    de::{
        read::{BorrowReader, Reader},
        Decoder,
    },
    error::DecodeError,
    BorrowDecode, Decode,
};
use std::fmt::Debug;

#[derive(Clone, Copy)]
enum Peek {
    Exact,
    Extra,
    None,
    ShortBatch,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
    peek: Peek,
}

impl Reader for Cursor<'_> {
    fn read(&mut self, output: &mut [u8]) -> Result<(), DecodeError> {
        if output.len() > self.bytes.len() - self.position {
            return Err(DecodeError::UnexpectedEnd {
                additional: output.len() - (self.bytes.len() - self.position),
            });
        }
        output.copy_from_slice(&self.bytes[self.position..self.position + output.len()]);
        self.position += output.len();
        Ok(())
    }

    fn peek_read(&mut self, length: usize) -> Option<&[u8]> {
        let remaining = &self.bytes[self.position..];
        match self.peek {
            Peek::None => None,
            Peek::Extra if remaining.len() >= length => Some(remaining),
            Peek::ShortBatch if length == 8 => remaining.get(..4),
            _ => remaining.get(..length),
        }
    }

    fn consume(&mut self, length: usize) {
        self.position = (self.position + length).min(self.bytes.len());
    }
}

impl<'a> BorrowReader<'a> for Cursor<'a> {
    fn take_bytes(&mut self, length: usize) -> Result<&'a [u8], DecodeError> {
        if length > self.bytes.len() - self.position {
            return Err(DecodeError::UnexpectedEnd {
                additional: length - (self.bytes.len() - self.position),
            });
        }
        let bytes = &self.bytes[self.position..self.position + length];
        self.position += length;
        Ok(bytes)
    }
}

fn scalar_into<T: Decode<()>, D: Decoder<Context = ()>>(
    decoder: &mut D,
    values: &mut Vec<T>,
) -> Result<(), DecodeError> {
    let len = usize::decode(decoder)?;
    decoder.claim_container_read::<T>(len)?;
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<T>());
        values.push(T::decode(decoder)?);
    }
    Ok(())
}

fn check<T, C>(bytes: &[u8], config: C, peek: Peek)
where
    T: Decode<()> + for<'a> BorrowDecode<'a, ()> + Debug + PartialEq,
    C: Config,
{
    let reader = || Cursor {
        bytes,
        position: 0,
        peek,
    };
    let mut reference = bincode::de::DecoderImpl::new(reader(), config, ());
    let mut expected = Vec::<T>::new();
    let result = scalar_into(&mut reference, &mut expected);
    let status = format!("{result:?}");
    let position = reference.reader().position;

    let mut decoder = bincode::de::DecoderImpl::new(reader(), config, ());
    let actual = Vec::<T>::decode(&mut decoder);
    assert_eq!(format!("{:?}", actual.as_ref().map(|_| ())), status);
    assert_eq!(decoder.reader().position, position);
    if let Ok(actual) = actual {
        assert_eq!(actual, expected);
    }

    let mut decoder = bincode::de::DecoderImpl::new(reader(), config, ());
    let actual = Vec::<T>::borrow_decode(&mut decoder);
    assert_eq!(format!("{:?}", actual.as_ref().map(|_| ())), status);
    assert_eq!(decoder.reader().position, position);
    if let Ok(actual) = actual {
        assert_eq!(actual, expected);
    }

    if matches!(peek, Peek::Exact) {
        let mut output = Vec::<T>::with_capacity(64);
        let allocation = output.as_ptr();
        let actual = bincode::decode_from_slice_into_vec(bytes, &mut output, config);
        assert_eq!(format!("{:?}", actual.as_ref().map(|_| ())), status);
        assert_eq!(output, expected);
        assert_eq!(output.as_ptr(), allocation);
        if let Ok(used) = actual {
            assert_eq!(used, position);
        }
    }
}

macro_rules! for_each_integer {
    ($function:ident) => {
        $function::<u16>();
        $function::<u32>();
        $function::<u64>();
        $function::<u128>();
        $function::<usize>();
        $function::<i16>();
        $function::<i32>();
        $function::<i64>();
        $function::<i128>();
        $function::<isize>();
    };
}

fn one_byte<T>()
where
    T: Decode<()> + for<'a> BorrowDecode<'a, ()> + Debug + PartialEq,
{
    let config = bincode::config::standard();
    for len in 0..=33 {
        let mut bytes = vec![len as u8];
        bytes.extend((0..len).map(|index| (index * 47 % 251) as u8));
        // Trailing bytes must not be interpreted as more vector elements.
        bytes.extend_from_slice(&[255; 17]);
        check::<T, _>(&bytes, config, Peek::Exact);
        check::<T, _>(&bytes, config.with_big_endian(), Peek::Exact);
    }
    for lane in 0..8 {
        for byte in 0..=250 {
            let mut bytes = vec![24u8];
            bytes.extend((0..24).map(|index| (index * 47 % 251) as u8));
            bytes[1 + lane] = byte;
            bytes[9 + lane] = byte;
            bytes[17 + lane] = byte;
            check::<T, _>(&bytes, config, Peek::Exact);
            check::<T, _>(&bytes, config.with_big_endian(), Peek::Exact);
        }
    }
}

#[test]
fn one_byte_varints_match_scalar_values_at_every_lane_and_tail() {
    for_each_integer!(one_byte);
}

fn malformed<T>()
where
    T: Decode<()> + for<'a> BorrowDecode<'a, ()> + Debug + PartialEq,
{
    let config = bincode::config::standard();
    for lane in 0..33 {
        for marker in 251..=255 {
            let mut bytes = vec![33u8];
            bytes.extend((0..33).map(|index| (index * 47 % 251) as u8));
            bytes[1 + lane] = marker;
            bytes.extend_from_slice(&[255; 17]);
            for end in [
                lane + 1,
                lane + 2,
                lane + 3,
                lane + 5,
                lane + 9,
                bytes.len(),
            ] {
                let bytes = &bytes[..end];
                check::<T, _>(bytes, config, Peek::Exact);
                check::<T, _>(bytes, config.with_big_endian(), Peek::Exact);
            }
        }
    }
}

#[test]
fn markers_and_truncation_preserve_errors_prefixes_and_reader_position() {
    for_each_integer!(malformed);
}

fn fallback<T>()
where
    T: Decode<()> + for<'a> BorrowDecode<'a, ()> + Debug + PartialEq,
{
    let config = bincode::config::standard();
    for marker in [0, 250, 251, 252, 253, 254, 255] {
        let mut bytes = vec![32u8];
        bytes.extend((0..32).map(|index| (index * 47 % 251) as u8));
        bytes[17] = marker;
        for peek in [Peek::None, Peek::Extra, Peek::ShortBatch] {
            check::<T, _>(&bytes, config, peek);
            check::<T, _>(&bytes, config.with_big_endian(), peek);
        }
        check::<T, _>(&bytes, config.with_limit::<7>(), Peek::Exact);
        check::<T, _>(&bytes, config.with_limit::<8>(), Peek::Exact);
        check::<T, _>(&bytes, config.with_limit::<128>(), Peek::Exact);
        check::<T, _>(&bytes, config.with_limit::<4096>(), Peek::Exact);
    }
}

#[test]
fn varint_batches_preserve_limits_and_custom_reader_fallbacks() {
    for_each_integer!(fallback);
}

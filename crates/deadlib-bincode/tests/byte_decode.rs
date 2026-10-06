use bincode::{
    config::Config,
    de::{
        read::{BorrowReader, Reader},
        DecoderImpl,
    },
    error::DecodeError,
    BorrowDecode, Decode,
};

struct NoPeek<'a>(&'a [u8]);
impl Reader for NoPeek<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        if bytes.len() > self.0.len() {
            return Err(DecodeError::UnexpectedEnd {
                additional: bytes.len() - self.0.len(),
            });
        }
        let (source, rest) = self.0.split_at(bytes.len());
        bytes.copy_from_slice(source);
        self.0 = rest;
        Ok(())
    }
}

impl<'a> BorrowReader<'a> for NoPeek<'a> {
    fn take_bytes(&mut self, len: usize) -> Result<&'a [u8], DecodeError> {
        if len > self.0.len() {
            return Err(DecodeError::UnexpectedEnd {
                additional: len - self.0.len(),
            });
        }
        let (source, rest) = self.0.split_at(len);
        self.0 = rest;
        Ok(source)
    }
}

struct PeekWhole<'a>(NoPeek<'a>);
impl Reader for PeekWhole<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        self.0.read(bytes)
    }
    fn peek_read(&mut self, _: usize) -> Option<&[u8]> {
        Some(self.0 .0)
    }
    fn consume(&mut self, len: usize) {
        self.0 .0 = &self.0 .0[len..];
    }
}

#[test]
fn byte_vector_readers_preserve_contents_and_consumption() {
    let config = bincode::config::standard();
    for len in [0, 1, 256] {
        let value = (0..len).map(|i| i as u8).collect::<Vec<_>>();
        let encoded = bincode::encode_to_vec((&value, 123u8), config).unwrap();
        let mut decoder = DecoderImpl::new(NoPeek(&encoded), config, ());
        assert_eq!(Vec::<u8>::borrow_decode(&mut decoder).unwrap(), value);
        assert_eq!(u8::borrow_decode(&mut decoder).unwrap(), 123);
        let mut decoder = DecoderImpl::new(PeekWhole(NoPeek(&encoded)), config, ());
        assert_eq!(Vec::<u8>::decode(&mut decoder).unwrap(), value);
        assert_eq!(u8::decode(&mut decoder).unwrap(), 123);
    }
    // A reader may expose fewer bytes than requested. Failure must not consume them.
    let mut decoder = DecoderImpl::new(PeekWhole(NoPeek(&[3, 17])), config, ());
    assert!(matches!(
        Vec::<u8>::decode(&mut decoder),
        Err(DecodeError::UnexpectedEnd { additional: 2 })
    ));
    assert_eq!(u8::decode(&mut decoder).unwrap(), 17);
}

fn check_byte_vectors(config: impl Config) {
    for len in [0, 1, 250, 251, 256, 65_535, 65_536] {
        let value = (0..len).map(|i| i as u8).collect::<Vec<_>>();
        let mut encoded = bincode::encode_to_vec(&value, config).unwrap();
        let used = encoded.len();
        encoded.extend_from_slice(b"trailing bytes");
        assert_eq!(
            bincode::decode_from_slice::<Vec<u8>, _>(&encoded, config).unwrap(),
            (value.clone(), used)
        );
        assert_eq!(
            bincode::borrow_decode_from_slice::<Vec<u8>, _>(&encoded, config).unwrap(),
            (value.clone(), used)
        );
        let mut decoder = DecoderImpl::new(NoPeek(&encoded), config, ());
        assert_eq!(Vec::<u8>::decode(&mut decoder).unwrap(), value);
        assert_eq!(u8::decode(&mut decoder).unwrap(), b't');
        let mut decoder =
            DecoderImpl::new(bincode::de::read::SliceReader::new(&encoded), config, ());
        assert_eq!(Vec::<u8>::borrow_decode(&mut decoder).unwrap(), value);
        assert_eq!(u8::borrow_decode(&mut decoder).unwrap(), b't');
    }
    let value = (0..256).map(|i| i as u8).collect::<Vec<_>>();
    let encoded = bincode::encode_to_vec(&value, config).unwrap();
    for end in 0..encoded.len() {
        let bytes = &encoded[..end];
        let mut decoder = DecoderImpl::new(NoPeek(bytes), config, ());
        let expected = Vec::<u8>::decode(&mut decoder).unwrap_err();
        let owned = bincode::decode_from_slice::<Vec<u8>, _>(bytes, config).unwrap_err();
        let borrowed = bincode::borrow_decode_from_slice::<Vec<u8>, _>(bytes, config).unwrap_err();
        assert_eq!(format!("{owned:?}"), format!("{expected:?}"));
        assert_eq!(format!("{borrowed:?}"), format!("{expected:?}"));
    }
}

#[test]
fn byte_vectors_preserve_wire_format_trailing_bytes_and_truncation_errors() {
    check_byte_vectors(bincode::config::standard());
    check_byte_vectors(bincode::config::standard().with_big_endian());
    check_byte_vectors(bincode::config::legacy());
    check_byte_vectors(bincode::config::legacy().with_big_endian());
}

#[test]
fn byte_vectors_preserve_allocation_limits() {
    let encoded = bincode::encode_to_vec(vec![0u8; 256], bincode::config::standard()).unwrap();
    let config = bincode::config::standard().with_limit::<256>();
    assert!(matches!(
        bincode::decode_from_slice::<Vec<u8>, _>(&encoded, config),
        Err(DecodeError::LimitExceeded)
    ));
    assert!(matches!(
        bincode::borrow_decode_from_slice::<Vec<u8>, _>(&encoded, config),
        Err(DecodeError::LimitExceeded)
    ));
}

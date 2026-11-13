//! Owned parser text with allocation-free storage for short tokens.
use super::fallible::SliceExt;
use crate::Error;
use std::{fmt, ops::Deref};

const INLINE: usize = size_of::<usize>() - 1;

pub(crate) struct Text(Data);
const _: () = assert!(size_of::<Text>() == size_of::<Box<[u8]>>());
enum Data {
    Inline { bytes: [u8; INLINE], len: u8 },
    Heap(Box<[u8]>),
}
impl Default for Text {
    fn default() -> Self {
        Self(Data::Inline {
            bytes: [0; INLINE],
            len: 0,
        })
    }
}
impl Text {
    pub(crate) fn new(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() <= INLINE {
            let mut value = [0; INLINE];
            value[..bytes.len()].copy_from_slice(bytes);
            Ok(Self(Data::Inline {
                bytes: value,
                len: bytes.len() as u8,
            }))
        } else {
            Ok(Self(Data::Heap(bytes.try_boxed()?)))
        }
    }
    pub(crate) fn try_clone(&self) -> Result<Self, Error> {
        Self::new(self)
    }
    pub(crate) fn heap_bytes(&self) -> usize {
        match &self.0 {
            Data::Inline { .. } => 0,
            Data::Heap(bytes) => bytes.len(),
        }
    }
}
impl From<Box<[u8]>> for Text {
    fn from(bytes: Box<[u8]>) -> Self {
        Self(Data::Heap(bytes))
    }
}
impl Deref for Text {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match &self.0 {
            Data::Inline { bytes, len } => &bytes[..usize::from(*len)],
            Data::Heap(bytes) => bytes,
        }
    }
}
impl AsRef<[u8]> for Text {
    fn as_ref(&self) -> &[u8] {
        self
    }
}
impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_ref(), f)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_storage_forms_preserve_bytes_and_existing_debug_output() {
        for bytes in [
            b"".as_slice(),
            b"a\0name\xff",
            b"abcdefgh",
            b"abcdefghijklmno",
            b"abcdefghijklmnop",
        ] {
            let text = Text::new(bytes).unwrap();
            assert_eq!(text.as_ref(), bytes);
            assert_eq!(format!("{text:?}"), format!("{:?}", bytes));
            assert_eq!(text.try_clone().unwrap().as_ref(), bytes);
            assert_eq!(
                text.heap_bytes(),
                if bytes.len() <= INLINE {
                    0
                } else {
                    bytes.len()
                }
            );
        }
    }
}

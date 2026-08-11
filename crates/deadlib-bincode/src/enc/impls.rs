use super::{write::Writer, Encode, Encoder};
use crate::{
    config::{Endianness, IntEncoding, InternalEndianConfig, InternalIntEncodingConfig},
    error::EncodeError,
};

impl Encode for () {
    fn encode<E: Encoder>(&self, _: &mut E) -> Result<(), EncodeError> {
        Ok(())
    }
}

impl Encode for bool {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        u8::from(*self).encode(encoder)
    }
}

impl Encode for u8 {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        encoder.writer().write(&[*self])
    }
}

impl Encode for u16 {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        match E::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_encode_u16(encoder.writer(), E::C::ENDIAN, *self)
            }
            IntEncoding::Fixed => match E::C::ENDIAN {
                Endianness::Big => encoder.writer().write(&self.to_be_bytes()),
                Endianness::Little => encoder.writer().write(&self.to_le_bytes()),
            },
        }
    }
}

impl Encode for u32 {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        match E::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_encode_u32(encoder.writer(), E::C::ENDIAN, *self)
            }
            IntEncoding::Fixed => match E::C::ENDIAN {
                Endianness::Big => encoder.writer().write(&self.to_be_bytes()),
                Endianness::Little => encoder.writer().write(&self.to_le_bytes()),
            },
        }
    }
}

impl Encode for u64 {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        match E::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_encode_u64(encoder.writer(), E::C::ENDIAN, *self)
            }
            IntEncoding::Fixed => match E::C::ENDIAN {
                Endianness::Big => encoder.writer().write(&self.to_be_bytes()),
                Endianness::Little => encoder.writer().write(&self.to_le_bytes()),
            },
        }
    }
}

impl Encode for u128 {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        match E::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_encode_u128(encoder.writer(), E::C::ENDIAN, *self)
            }
            IntEncoding::Fixed => match E::C::ENDIAN {
                Endianness::Big => encoder.writer().write(&self.to_be_bytes()),
                Endianness::Little => encoder.writer().write(&self.to_le_bytes()),
            },
        }
    }
}

impl Encode for usize {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        match E::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_encode_usize(encoder.writer(), E::C::ENDIAN, *self)
            }
            IntEncoding::Fixed => match E::C::ENDIAN {
                Endianness::Big => encoder.writer().write(&(*self as u64).to_be_bytes()),
                Endianness::Little => encoder.writer().write(&(*self as u64).to_le_bytes()),
            },
        }
    }
}

impl Encode for i8 {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        encoder.writer().write(&[*self as u8])
    }
}

impl Encode for i16 {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        match E::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_encode_i16(encoder.writer(), E::C::ENDIAN, *self)
            }
            IntEncoding::Fixed => match E::C::ENDIAN {
                Endianness::Big => encoder.writer().write(&self.to_be_bytes()),
                Endianness::Little => encoder.writer().write(&self.to_le_bytes()),
            },
        }
    }
}

impl Encode for i32 {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        match E::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_encode_i32(encoder.writer(), E::C::ENDIAN, *self)
            }
            IntEncoding::Fixed => match E::C::ENDIAN {
                Endianness::Big => encoder.writer().write(&self.to_be_bytes()),
                Endianness::Little => encoder.writer().write(&self.to_le_bytes()),
            },
        }
    }
}

impl Encode for i64 {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        match E::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_encode_i64(encoder.writer(), E::C::ENDIAN, *self)
            }
            IntEncoding::Fixed => match E::C::ENDIAN {
                Endianness::Big => encoder.writer().write(&self.to_be_bytes()),
                Endianness::Little => encoder.writer().write(&self.to_le_bytes()),
            },
        }
    }
}

impl Encode for i128 {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        match E::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_encode_i128(encoder.writer(), E::C::ENDIAN, *self)
            }
            IntEncoding::Fixed => match E::C::ENDIAN {
                Endianness::Big => encoder.writer().write(&self.to_be_bytes()),
                Endianness::Little => encoder.writer().write(&self.to_le_bytes()),
            },
        }
    }
}

impl Encode for isize {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        match E::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_encode_isize(encoder.writer(), E::C::ENDIAN, *self)
            }
            IntEncoding::Fixed => match E::C::ENDIAN {
                Endianness::Big => encoder.writer().write(&(*self as i64).to_be_bytes()),
                Endianness::Little => encoder.writer().write(&(*self as i64).to_le_bytes()),
            },
        }
    }
}

impl Encode for f32 {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        match E::C::ENDIAN {
            Endianness::Big => encoder.writer().write(&self.to_be_bytes()),
            Endianness::Little => encoder.writer().write(&self.to_le_bytes()),
        }
    }
}

impl Encode for f64 {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        match E::C::ENDIAN {
            Endianness::Big => encoder.writer().write(&self.to_be_bytes()),
            Endianness::Little => encoder.writer().write(&self.to_le_bytes()),
        }
    }
}

impl<T> Encode for [T]
where
    T: Encode,
{
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        super::encode_slice_len(encoder, self.len())?;

        if crate::unty::type_equal::<T, u8>() {
            // SAFETY: T = u8
            let t: &[u8] = unsafe { core::mem::transmute(self) };
            encoder.writer().write(t)?;
            return Ok(());
        }

        for item in self {
            item.encode(encoder)?;
        }
        Ok(())
    }
}

impl Encode for str {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        self.as_bytes().encode(encoder)
    }
}

impl<T, const N: usize> Encode for [T; N]
where
    T: Encode,
{
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        if crate::unty::type_equal::<T, u8>() {
            // SAFETY: this is &[u8; N]
            let array_slice: &[u8] =
                unsafe { core::slice::from_raw_parts(self.as_ptr().cast(), N) };
            encoder.writer().write(array_slice)
        } else {
            for item in self.iter() {
                item.encode(encoder)?;
            }
            Ok(())
        }
    }
}

impl<T> Encode for Option<T>
where
    T: Encode,
{
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        super::encode_option_variant(encoder, self)?;
        if let Some(val) = self {
            val.encode(encoder)?;
        }
        Ok(())
    }
}

impl<T> Encode for &T
where
    T: Encode + ?Sized,
{
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        T::encode(self, encoder)
    }
}

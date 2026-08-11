use super::{BorrowDecode, BorrowDecoder, Decode, Decoder};
use crate::error::DecodeError;

macro_rules! impl_tuple {
    () => {};
    ($first:ident $(, $extra:ident)*) => {
        impl<'de, $first $(, $extra)*, Context> BorrowDecode<'de, Context> for ($first, $($extra, )*)
        where
            $first: BorrowDecode<'de, Context>,
        $(
            $extra : BorrowDecode<'de, Context>,
        )*
         {
            fn borrow_decode<BD: BorrowDecoder<'de, Context = Context>>(decoder: &mut BD) -> Result<Self, DecodeError> {
                Ok((
                    $first::borrow_decode(decoder)?,
                    $($extra :: borrow_decode(decoder)?, )*
                ))
            }
        }

        impl<Context, $first $(, $extra)*> Decode<Context> for ($first, $($extra, )*)
        where
            $first: Decode<Context>,
        $(
            $extra : Decode<Context>,
        )*
        {
            fn decode<DE: Decoder<Context = Context>>(decoder: &mut DE) -> Result<Self, DecodeError> {
                Ok((
                    $first::decode(decoder)?,
                    $($extra :: decode(decoder)?, )*
                ))
            }
        }
    }
}

impl_tuple!(A);
impl_tuple!(A, B);
impl_tuple!(A, B, C);

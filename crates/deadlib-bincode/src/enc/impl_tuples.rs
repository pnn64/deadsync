use super::{Encode, Encoder};
use crate::error::EncodeError;

impl<A: Encode> Encode for (A,) {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        self.0.encode(encoder)
    }
}

impl<A: Encode, B: Encode> Encode for (A, B) {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)
    }
}

impl<A: Encode, B: Encode, C: Encode> Encode for (A, B, C) {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)
    }
}

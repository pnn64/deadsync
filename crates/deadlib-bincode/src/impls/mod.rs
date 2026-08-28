mod alloc;
pub(crate) use self::alloc::decode_vec_into;
pub use self::alloc::{encode_into_vec, encode_to_vec};

mod std;

mod alloc;
pub(crate) use self::alloc::{borrow_decode_vec_into, decode_string_into, decode_vec_into};
pub use self::alloc::{encode_into_vec, encode_to_vec};

mod std;
pub(crate) use self::std::{
    borrow_decode_hash_map_into, borrow_decode_hash_set_into, decode_hash_map_into,
    decode_hash_set_into,
};

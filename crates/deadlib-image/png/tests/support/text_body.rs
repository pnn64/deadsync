//! Independent tEXt/zTXt framing for text-body validation and recovery tests.
use super::chunks;
use deadlib_image_png::{AncillaryChunk, ChunkPosition};

pub fn raw(text: &[u8], compressed: bool, position: ChunkPosition) -> AncillaryChunk {
    let mut data = b"Body\0".to_vec();
    if compressed {
        data.push(0);
        data.extend(chunks::stored(text));
    } else {
        data.extend(text);
    }
    AncillaryChunk {
        kind: if compressed { *b"zTXt" } else { *b"tEXt" },
        data,
        position,
    }
}

pub fn fixture(text: &[u8], compressed: bool, position: ChunkPosition) -> Vec<u8> {
    let chunk = raw(text, compressed, position);
    chunks::fixture(chunk.kind, &chunk.data, position)
}

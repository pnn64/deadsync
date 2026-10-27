//! Native packed prefix trees for the ISO Layer III pair and count1 codebooks.
use crate::{Error, bits::BitReader};
#[path = "huffman_tables.rs"]
mod tables;

fn symbol(bits: &mut BitReader<'_>, mut node: u16) -> Result<u8, Error> {
    loop {
        let next = tables::NODES[node as usize][bits.read(1)? as usize];
        if next & 0x8000 != 0 {
            return Ok(next as u8);
        }
        node = next;
    }
}
fn signed(bits: &mut BitReader<'_>, magnitude: u16) -> Result<i16, Error> {
    if magnitude != 0 && bits.flag()? {
        Ok(-(magnitude as i16))
    } else {
        Ok(magnitude as i16)
    }
}

/// Decodes one pair, including linbits and signs. Failure is cursor-atomic.
/// Book zero emits two zeros without consuming bits. Books 4 and 14 are invalid
/// only when a coded pair actually requests them; unused region ids are ignored.
pub fn pair(bits: &mut BitReader<'_>, book: u8) -> Result<[i16; 2], Error> {
    if book == 0 {
        return Ok([0; 2]);
    }
    if book >= 32 || book == 4 || book == 14 {
        return Err(Error::Invalid("Layer III pair codebook"));
    }
    let mut cursor = *bits;
    let value = symbol(&mut cursor, tables::ROOTS[book as usize])?;
    let lin = tables::LINBITS[book as usize] as u32;
    let mut x = (value >> 4) as u16;
    let mut y = (value & 15) as u16;
    if x == 15 && lin != 0 {
        x += cursor.read(lin)? as u16;
    }
    let x = signed(&mut cursor, x)?;
    if y == 15 && lin != 0 {
        y += cursor.read(lin)? as u16;
    }
    let y = signed(&mut cursor, y)?;
    *bits = cursor;
    Ok([x, y])
}

/// Decodes a count1 quadruple and its nonzero signs. An incomplete final symbol
/// leaves the cursor untouched so the granule parser can discard stuffing bits.
pub fn quad(bits: &mut BitReader<'_>, table_b: bool) -> Result<[i16; 4], Error> {
    let mut cursor = *bits;
    let value = symbol(&mut cursor, tables::ROOTS[32 + usize::from(table_b)])?;
    let mut output = [0; 4];
    for (n, sample) in output.iter_mut().enumerate() {
        *sample = signed(&mut cursor, ((value >> (3 - n)) & 1) as u16)?;
    }
    *bits = cursor;
    Ok(output)
}

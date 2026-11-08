//! Native packed prefix trees for the ISO Layer III pair and count1 codebooks.
use crate::{Error, bits::BitReader};
#[path = "huffman_tables.rs"]
mod tables;

#[inline(always)]
fn symbol(bits: &mut BitReader<'_>, mut node: u16) -> Result<u8, Error> {
    loop {
        let next = tables::NODES[node as usize][bits.read(1)? as usize];
        if next & 0x8000 != 0 {
            return Ok(next as u8);
        }
        node = next;
    }
}
#[inline(always)]
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
#[inline(always)]
pub fn pair(bits: &mut BitReader<'_>, book: u8) -> Result<[i16; 2], Error> {
    if book == 0 {
        return Ok([0; 2]);
    }
    if book >= 32 || book == 4 || book == 14 {
        return Err(Error::Invalid("Layer III pair codebook"));
    }
    if matches!(book, 2 | 3) {
        let entry = if book == 2 {
            let mut cursor = *bits;
            if cursor.flag()? {
                *bits = cursor;
                return Ok([0; 2]);
            }
            let entry = tables::PAIR2[cursor.peek8() >> 1];
            cursor.skip((entry >> 8) as usize - 1)?;
            *bits = cursor;
            entry
        } else {
            let entry = tables::PAIR3[bits.peek8()];
            bits.skip((entry >> 8) as usize)?;
            entry
        };
        return Ok([((entry << 13) as i16) >> 13, ((entry << 10) as i16) >> 13]);
    }
    let mut cursor = *bits;
    if book == 1 {
        let entry = tables::PAIR1[cursor.peek5()];
        cursor.skip((entry >> 16) as usize)?;
        *bits = cursor;
        return Ok([entry as u8 as i8 as i16, (entry >> 8) as u8 as i8 as i16]);
    }
    let value = if book >= 24 {
        let entry = tables::ESCAPE_PREFIX[cursor.peek5()];
        if entry & 0x8000 != 0 {
            cursor.skip(((entry >> 8) & 7) as usize)?;
            entry as u8
        } else {
            cursor.skip(5)?;
            symbol(&mut cursor, entry)?
        }
    } else {
        symbol(&mut cursor, tables::ROOTS[book as usize])?
    };
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
#[inline(always)]
pub fn quad(bits: &mut BitReader<'_>, table_b: bool) -> Result<[i16; 4], Error> {
    if table_b {
        let prefix = bits.peek8();
        if prefix >= 240 {
            bits.skip(4)?;
            return Ok([0; 4]);
        }
        let entry = tables::COUNT1_B[prefix];
        bits.skip((entry >> 8) as usize)?;
        return Ok(tables::COUNT1_SIGNS[(entry & 255) as usize]);
    }
    let mut cursor = *bits;
    let value = if cursor.flag()? {
        0
    } else {
        let entry = tables::COUNT1[cursor.peek5()];
        cursor.skip((entry >> 8) as usize)?;
        entry as u8
    };
    let layout = tables::SIGN_LAYOUT[value as usize];
    let signs = cursor.read(u32::from(layout & 7))? as usize;
    let output = tables::COUNT1_SIGNS[usize::from(layout >> 3) + signs];
    *bits = cursor;
    Ok(output)
}

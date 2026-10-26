use crate::{
    Budget, Error, Identification, Limits,
    bitpack::{BitReader, ilog},
    codebook::{Codebook, power_at_most},
    headers::header,
    reserve,
};

pub struct Setup {
    channels: u8,
    block_sizes: [usize; 2],
    codebooks: Vec<Codebook>,
    floors: Vec<Floor>,
    residues: Vec<Residue>,
    mappings: Vec<Mapping>,
    modes: Vec<Mode>,
}
pub enum Floor {
    Type0(Floor0),
    Type1(Floor1),
}
pub struct Floor0 {
    pub order: u8,
    pub rate: u16,
    pub bark_map_size: u16,
    pub amplitude_bits: u8,
    pub amplitude_offset: u8,
    pub books: Vec<u8>,
}
pub struct FloorClass {
    pub dimensions: u8,
    pub subclasses: u8,
    pub masterbook: Option<u8>,
    pub books: [Option<u8>; 8],
}
pub struct Floor1 {
    pub partition_classes: Vec<u8>,
    pub classes: Vec<FloorClass>,
    pub multiplier: u8,
    pub x: Vec<u16>,
    /// Nearest lower/higher X among earlier entries, in bitstream order.
    pub neighbors: Vec<[u8; 2]>,
    pub sorted_order: Vec<u8>,
}
pub struct Residue {
    pub kind: u8,
    pub begin: u32,
    pub end: u32,
    pub partition_size: u32,
    pub classbook: u8,
    pub books: Vec<[Option<u8>; 8]>,
}
pub struct Mapping {
    /// Ordered magnitude/angle channel pairs; undo in reverse order.
    pub coupling: Vec<[u8; 2]>,
    pub mux: Vec<u8>,
    pub submaps: Vec<Submap>,
}
#[derive(Clone, Copy, Debug)]
pub struct Submap {
    pub floor: u8,
    pub residue: u8,
}
#[derive(Clone, Copy, Debug)]
pub struct Mode {
    pub long_block: bool,
    pub mapping: u8,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioHeader {
    pub mode: u8,
    pub block_size: usize,
    pub previous_long: bool,
    pub next_long: bool,
}

impl Setup {
    pub fn parse(
        packet: &[u8],
        identification: &Identification,
        limits: &Limits,
    ) -> Result<Self, Error> {
        if packet.len() > limits.max_packet_bytes {
            return Err(Error::LimitExceeded("setup packet bytes"));
        }
        // Validate caller-constructed identification as well as parsed values.
        identification.validate()?;
        let mut bits = BitReader::new(header(packet, 5)?);
        let mut budget = Budget::new(limits);
        let count = bits.read(8)? as usize + 1;
        let mut codebooks = storage(count, &mut budget)?;
        for _ in 0..count {
            codebooks.push(Codebook::read_with_budget(&mut bits, limits, &mut budget)?);
        }
        let times = bits.read(6)? + 1;
        for _ in 0..times {
            if bits.read(16)? != 0 {
                return Err(Error::Invalid("time transform type"));
            }
        }
        let count = bits.read(6)? as usize + 1;
        let mut floors = storage(count, &mut budget)?;
        for _ in 0..count {
            floors.push(match bits.read(16)? {
                0 => Floor::Type0(read_floor0(&mut bits, &codebooks, &mut budget)?),
                1 => Floor::Type1(read_floor1(&mut bits, &codebooks, &mut budget)?),
                _ => return Err(Error::Invalid("floor type")),
            });
        }
        let count = bits.read(6)? as usize + 1;
        let mut residues = storage(count, &mut budget)?;
        for _ in 0..count {
            residues.push(read_residue(&mut bits, &codebooks, &mut budget)?);
        }
        let count = bits.read(6)? as usize + 1;
        let mut mappings = storage(count, &mut budget)?;
        for _ in 0..count {
            mappings.push(read_mapping(
                &mut bits,
                identification.channels,
                floors.len(),
                residues.len(),
                &mut budget,
            )?);
        }
        let count = bits.read(6)? as usize + 1;
        let mut modes = storage(count, &mut budget)?;
        for _ in 0..count {
            let long_block = bits.flag()?;
            if bits.read(16)? != 0 {
                return Err(Error::Invalid("mode window type"));
            }
            if bits.read(16)? != 0 {
                return Err(Error::Invalid("mode transform type"));
            }
            let mapping = bits.read(8)? as u8;
            if mapping as usize >= mappings.len() {
                return Err(Error::Invalid("mode mapping"));
            }
            modes.push(Mode {
                long_block,
                mapping,
            });
        }
        if !bits.flag()? {
            return Err(Error::Invalid("setup framing bit"));
        }
        Ok(Self {
            channels: identification.channels,
            block_sizes: identification.block_sizes,
            codebooks,
            floors,
            residues,
            mappings,
            modes,
        })
    }
    pub fn channels(&self) -> u8 {
        self.channels
    }
    /// Owned setup elements, excluding allocator and parent struct metadata.
    pub fn storage_bytes(&self) -> usize {
        fn size<T>(v: &Vec<T>) -> usize {
            v.capacity() * core::mem::size_of::<T>()
        }
        size(&self.codebooks)
            + self
                .codebooks
                .iter()
                .map(Codebook::storage_bytes)
                .sum::<usize>()
            + size(&self.floors)
            + self
                .floors
                .iter()
                .map(|floor| match floor {
                    Floor::Type0(value) => size(&value.books),
                    Floor::Type1(value) => {
                        size(&value.partition_classes)
                            + size(&value.classes)
                            + size(&value.x)
                            + size(&value.neighbors)
                            + size(&value.sorted_order)
                    }
                })
                .sum::<usize>()
            + size(&self.residues)
            + self.residues.iter().map(|r| size(&r.books)).sum::<usize>()
            + size(&self.mappings)
            + self
                .mappings
                .iter()
                .map(|m| size(&m.coupling) + size(&m.mux) + size(&m.submaps))
                .sum::<usize>()
            + size(&self.modes)
    }
    pub fn block_sizes(&self) -> [usize; 2] {
        self.block_sizes
    }
    pub fn codebooks(&self) -> &[Codebook] {
        &self.codebooks
    }
    pub fn floors(&self) -> &[Floor] {
        &self.floors
    }
    pub fn residues(&self) -> &[Residue] {
        &self.residues
    }
    pub fn mappings(&self) -> &[Mapping] {
        &self.mappings
    }
    pub fn modes(&self) -> &[Mode] {
        &self.modes
    }
    /// Read only the audio packet's mode/window prefix, leaving the reader at
    /// the floor payload. Non-audio packets must be ignored by a stream decoder.
    pub fn read_audio_header(&self, bits: &mut BitReader<'_>) -> Result<AudioHeader, Error> {
        if bits.flag()? {
            return Err(Error::Invalid("non-audio packet"));
        }
        let mode = bits.read(ilog(self.modes.len() as u32 - 1))? as u8;
        let config = self
            .modes
            .get(mode as usize)
            .ok_or(Error::Invalid("audio mode number"))?;
        let (previous_long, next_long) = if config.long_block {
            (bits.flag()?, bits.flag()?)
        } else {
            (false, false)
        };
        Ok(AudioHeader {
            mode,
            block_size: self.block_sizes[config.long_block as usize],
            previous_long,
            next_long,
        })
    }
}

fn storage<T>(count: usize, budget: &mut Budget) -> Result<Vec<T>, Error> {
    budget.charge(
        count
            .checked_mul(core::mem::size_of::<T>())
            .ok_or(Error::LimitExceeded("setup memory"))?,
    )?;
    let mut out = Vec::new();
    reserve(&mut out, count)?;
    Ok(out)
}
fn book_index(bits: &mut BitReader<'_>, books: &[Codebook], vq: bool) -> Result<u8, Error> {
    let index = bits.read(8)? as u8;
    let book = books
        .get(index as usize)
        .ok_or(Error::Invalid("codebook index"))?;
    if vq && book.lookup_type() == 0 {
        return Err(Error::Invalid("VQ codebook has no lookup"));
    }
    Ok(index)
}
fn read_floor0(
    bits: &mut BitReader<'_>,
    books: &[Codebook],
    budget: &mut Budget,
) -> Result<Floor0, Error> {
    let order = bits.read(8)? as u8;
    let rate = bits.read(16)? as u16;
    let bark_map_size = bits.read(16)? as u16;
    let amplitude_bits = bits.read(6)? as u8;
    let amplitude_offset = bits.read(8)? as u8;
    let count = bits.read(4)? as usize + 1;
    if order == 0 || rate == 0 || bark_map_size == 0 {
        return Err(Error::Invalid("zero floor0 order/rate/bark size"));
    }
    let mut floor_books = storage(count, budget)?;
    for _ in 0..count {
        floor_books.push(book_index(bits, books, true)?);
    }
    Ok(Floor0 {
        order,
        rate,
        bark_map_size,
        amplitude_bits,
        amplitude_offset,
        books: floor_books,
    })
}
fn read_floor1(
    bits: &mut BitReader<'_>,
    books: &[Codebook],
    budget: &mut Budget,
) -> Result<Floor1, Error> {
    let count = bits.read(5)? as usize;
    let mut partition_classes = storage(count, budget)?;
    for _ in 0..count {
        partition_classes.push(bits.read(4)? as u8);
    }
    let class_count = partition_classes
        .iter()
        .max()
        .map_or(0, |&n| n as usize + 1);
    let mut classes = storage(class_count, budget)?;
    for _ in 0..class_count {
        let dimensions = bits.read(3)? as u8 + 1;
        let subclasses = bits.read(2)? as u8;
        let masterbook = if subclasses != 0 {
            Some(book_index(bits, books, false)?)
        } else {
            None
        };
        let mut subclass_books = [None; 8];
        for slot in subclass_books.iter_mut().take(1 << subclasses) {
            let index = bits.read(8)? as usize;
            if index != 0 {
                if index > books.len() {
                    return Err(Error::Invalid("floor subclass book"));
                }
                *slot = Some((index - 1) as u8);
            }
        }
        classes.push(FloorClass {
            dimensions,
            subclasses,
            masterbook,
            books: subclass_books,
        });
    }
    let multiplier = bits.read(2)? as u8 + 1;
    let range_bits = bits.read(4)?;
    let values = 2 + partition_classes
        .iter()
        .map(|&n| classes[n as usize].dimensions as usize)
        .sum::<usize>();
    if values > 65 {
        return Err(Error::Invalid("floor1 has more than 65 points"));
    }
    let mut x = storage(values, budget)?;
    x.push(0);
    x.push(1u16 << range_bits);
    for &class in &partition_classes {
        for _ in 0..classes[class as usize].dimensions {
            x.push(bits.read(range_bits)? as u16);
        }
    }
    let mut sorted_order = storage(values, budget)?;
    sorted_order.extend((0..values).map(|n| n as u8));
    sorted_order.sort_unstable_by_key(|&n| x[n as usize]);
    for pair in sorted_order.windows(2) {
        if x[pair[0] as usize] == x[pair[1] as usize] {
            return Err(Error::Invalid("duplicate floor1 X position"));
        }
    }
    let mut neighbors = storage(values, budget)?;
    neighbors.resize(values, [0, 1]);
    for i in 2..values {
        let low = (0..i)
            .filter(|&n| x[n] < x[i])
            .max_by_key(|&n| x[n])
            .ok_or(Error::Invalid("floor1 low neighbor"))?;
        let high = (0..i)
            .filter(|&n| x[n] > x[i])
            .min_by_key(|&n| x[n])
            .ok_or(Error::Invalid("floor1 high neighbor"))?;
        neighbors[i] = [low as u8, high as u8];
    }
    Ok(Floor1 {
        partition_classes,
        classes,
        multiplier,
        x,
        neighbors,
        sorted_order,
    })
}
fn read_residue(
    bits: &mut BitReader<'_>,
    books: &[Codebook],
    budget: &mut Budget,
) -> Result<Residue, Error> {
    let kind = bits.read(16)?;
    if kind > 2 {
        return Err(Error::Invalid("residue type"));
    }
    let begin = bits.read(24)?;
    let end = bits.read(24)?;
    let partition_size = bits.read(24)? + 1;
    if begin > end {
        return Err(Error::Invalid("residue range"));
    }
    let classifications = bits.read(6)? as usize + 1;
    let classbook = book_index(bits, books, false)?;
    let class_config = &books[classbook as usize];
    if !power_at_most(
        classifications as u32,
        class_config.dimensions() as u32,
        class_config.entries() as u32,
    ) {
        return Err(Error::Invalid(
            "residue classifications exceed classbook entries",
        ));
    }
    let mut cascades = [0; 64];
    for cascade in &mut cascades[..classifications] {
        let low = bits.read(3)? as u8;
        let high = if bits.flag()? { bits.read(5)? as u8 } else { 0 };
        *cascade = low | high << 3;
    }
    let mut residue_books = storage(classifications, budget)?;
    for &cascade in &cascades[..classifications] {
        let mut passes = [None; 8];
        for (pass, slot) in passes.iter_mut().enumerate() {
            if cascade & (1 << pass) == 0 {
                continue;
            }
            let book = book_index(bits, books, true)?;
            if !(partition_size as usize).is_multiple_of(books[book as usize].dimensions()) {
                return Err(Error::Invalid(
                    "residue partition is not a codebook dimension multiple",
                ));
            }
            *slot = Some(book);
        }
        residue_books.push(passes);
    }
    Ok(Residue {
        kind: kind as u8,
        begin,
        end,
        partition_size,
        classbook,
        books: residue_books,
    })
}
fn read_mapping(
    bits: &mut BitReader<'_>,
    channels: u8,
    floors: usize,
    residues: usize,
    budget: &mut Budget,
) -> Result<Mapping, Error> {
    if bits.read(16)? != 0 {
        return Err(Error::Invalid("mapping type"));
    }
    let count = if bits.flag()? {
        bits.read(4)? as usize + 1
    } else {
        1
    };
    let steps = if bits.flag()? {
        bits.read(8)? as usize + 1
    } else {
        0
    };
    let mut coupling = storage(steps, budget)?;
    for _ in 0..steps {
        let magnitude = bits.read(ilog(channels as u32 - 1))? as u8;
        let angle = bits.read(ilog(channels as u32 - 1))? as u8;
        if magnitude == angle || magnitude >= channels || angle >= channels {
            return Err(Error::Invalid("coupling channels"));
        }
        coupling.push([magnitude, angle]);
    }
    if bits.read(2)? != 0 {
        return Err(Error::Invalid("mapping reserved bits"));
    }
    let mut mux = storage(channels as usize, budget)?;
    for _ in 0..channels {
        let submap = if count > 1 { bits.read(4)? as u8 } else { 0 };
        if submap as usize >= count {
            return Err(Error::Invalid("channel submap"));
        }
        mux.push(submap);
    }
    let mut submaps = storage(count, budget)?;
    for _ in 0..count {
        bits.read(8)?; // The unused time placeholder is explicitly ignored.
        let floor = bits.read(8)? as u8;
        let residue = bits.read(8)? as u8;
        if floor as usize >= floors || residue as usize >= residues {
            return Err(Error::Invalid("submap floor/residue index"));
        }
        submaps.push(Submap { floor, residue });
    }
    Ok(Mapping {
        coupling,
        mux,
        submaps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitpack::BitWriter;
    #[test]
    fn floor1_sixty_five_point_limit_is_a_format_constraint() {
        for count in [65, 66] {
            let mut bits = BitWriter::new();
            bits.write(8, 5).unwrap();
            for i in 0..8 {
                bits.write(u32::from(i == 7 && count == 65), 4).unwrap();
            }
            for dimensions in if count == 65 {
                &[8u32, 7][..]
            } else {
                &[8][..]
            } {
                bits.write(dimensions - 1, 3).unwrap();
                bits.write(0, 2).unwrap();
                bits.write(0, 8).unwrap();
            }
            bits.write(0, 2).unwrap();
            bits.write(7, 4).unwrap();
            for x in 1..count - 1 {
                bits.write(x, 7).unwrap();
            }
            let bytes = bits.finish().unwrap();
            let limits = Limits::default();
            let result = read_floor1(&mut BitReader::new(&bytes), &[], &mut Budget::new(&limits));
            if count == 65 {
                assert_eq!(result.unwrap().x.len(), 65);
            } else {
                assert!(matches!(
                    result,
                    Err(Error::Invalid("floor1 has more than 65 points"))
                ));
            }
        }
    }
}

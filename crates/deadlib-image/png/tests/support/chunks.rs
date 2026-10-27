//! Independent PNG wire constructors shared by integration tests and fuzz seeds.
use deadlib_image_png::ChunkPosition;

pub const PIXEL: [u8; 3] = [17, 34, 51];
pub const POSITIONS: [ChunkPosition; 3] = [
    ChunkPosition::BeforePalette,
    ChunkPosition::BeforeData,
    ChunkPosition::AfterData,
];
pub const STANDARD_NAMES: [[u8; 4]; 25] = [
    *b"IHDR", *b"PLTE", *b"IDAT", *b"IEND", *b"tRNS", *b"cHRM", *b"gAMA", *b"iCCP", *b"sBIT",
    *b"sRGB", *b"cICP", *b"mDCV", *b"cLLI", *b"bKGD", *b"hIST", *b"pHYs", *b"sPLT", *b"eXIf",
    *b"tIME", *b"tEXt", *b"zTXt", *b"iTXt", *b"acTL", *b"fcTL", *b"fdAT",
];

pub fn crc(bytes: &[u8]) -> u32 {
    let mut state = !0u32;
    for &byte in bytes {
        state ^= u32::from(byte);
        for _ in 0..8 {
            state = (state >> 1) ^ if state & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    !state
}

pub fn chunk(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend(kind);
    out.extend(data);
    out.extend(crc(&out[4..]).to_be_bytes());
    out
}

pub fn header(color: u8, depth: u8) -> Vec<u8> {
    chunk(*b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, depth, color, 0, 0, 0])
}

pub fn stored(raw: &[u8]) -> Vec<u8> {
    let len = u16::try_from(raw.len()).unwrap();
    let mut wire = vec![0x78, 0x01, 0x01];
    wire.extend(len.to_le_bytes());
    wire.extend((!len).to_le_bytes());
    wire.extend(raw);
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in raw {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    wire.extend(((b << 16) | a).to_be_bytes());
    wire
}

pub fn file(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut wire = b"\x89PNG\r\n\x1a\n".to_vec();
    for bytes in chunks {
        wire.extend(bytes);
    }
    wire
}

pub fn fixture(kind: [u8; 4], data: &[u8], position: ChunkPosition) -> Vec<u8> {
    let mut chunks = vec![header(2, 8)];
    let extra = chunk(kind, data);
    if position == ChunkPosition::BeforePalette {
        chunks.push(extra.clone());
    }
    chunks.push(chunk(*b"PLTE", &[0, 0, 0, 255, 255, 255]));
    if position == ChunkPosition::BeforeData {
        chunks.push(extra.clone());
    }
    chunks.push(chunk(*b"IDAT", &stored(&[0, PIXEL[0], PIXEL[1], PIXEL[2]])));
    if position == ChunkPosition::AfterData {
        chunks.push(extra);
    }
    chunks.push(chunk(*b"IEND", &[]));
    file(&chunks)
}

pub fn property_name(base: [u8; 4], bits: u8) -> [u8; 4] {
    std::array::from_fn(|i| (base[i] & !32) | (((bits >> i) & 1) << 5))
}

pub fn permutations(values: &mut [usize], start: usize, visit: &mut impl FnMut(&[usize])) {
    if start == values.len() {
        visit(values);
    } else {
        for i in start..values.len() {
            values.swap(start, i);
            permutations(values, start + 1, visit);
            values.swap(start, i);
        }
    }
}

pub fn core_chunks() -> [Vec<u8>; 6] {
    let zlib = stored(&[0, 0]); // One 1-bit palette index, with zero row padding.
    [
        header(3, 1),
        chunk(*b"PLTE", &[0, 0, 0, 255, 255, 255]),
        chunk(*b"tRNS", &[0, 255]),
        chunk(*b"IDAT", &zlib[..5]),
        chunk(*b"IDAT", &zlib[5..]),
        chunk(*b"IEND", &[]),
    ]
}

/// Fresh deterministic wire fixtures, including malformed core permutations.
#[allow(dead_code)] // The integration test imports individual constructors.
pub fn seeds() -> Vec<(String, Vec<u8>)> {
    let mut cases = Vec::new();
    for bits in 0..16 {
        for (slot, position) in POSITIONS.into_iter().enumerate() {
            cases.push((
                format!("properties-{bits}-{slot}"),
                fixture(property_name(*b"vPAg", bits), &[0, 255, 128], position),
            ));
        }
    }
    for canonical in STANDARD_NAMES {
        for bits in 0..16 {
            let variant = property_name(canonical, bits);
            if variant == canonical {
                continue;
            }
            for (slot, position) in POSITIONS.into_iter().enumerate() {
                let name = std::str::from_utf8(&variant).unwrap();
                cases.push((
                    format!("case-{name}-{slot}"),
                    fixture(variant, &[255, 0, 128], position),
                ));
            }
        }
    }
    let chunks = core_chunks();
    permutations(&mut [0, 1, 2, 3, 4, 5], 0, &mut |order| {
        let label: String = order.iter().map(|i| char::from(b'0' + *i as u8)).collect();
        let wire = file(&order.iter().map(|&i| chunks[i].clone()).collect::<Vec<_>>());
        cases.push((format!("order-{label}"), wire));
    });
    cases
}

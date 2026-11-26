//! MPEG-4 Visual and H.263 variable-length code tables.
use crate::bitstream::vlc_table;

#[rustfmt::skip]
pub(crate) static INTRA_MB: [u16; 512] = vlc_table(&[
    (0x1, 1), (0x1, 3), (0x2, 3), (0x3, 3), (0x1, 4), (0x1, 6), (0x2, 6), (0x3, 6),
    (0x1, 9),
]);

#[rustfmt::skip]
pub(crate) static INTER_MB: [u16; 8192] = vlc_table(&[
    (0x1, 1), (0x3, 4), (0x2, 4), (0x5, 6), (0x3, 5), (0x4, 8), (0x3, 8), (0x3, 7),
    (0x3, 3), (0x7, 7), (0x6, 7), (0x5, 9), (0x4, 6), (0x4, 9), (0x3, 9), (0x2, 9),
    (0x2, 3), (0x5, 7), (0x4, 7), (0x5, 8), (0x1, 9), (0x0, 0), (0x0, 0), (0x0, 0),
    (0x2, 11), (0xc, 13), (0xe, 13), (0xf, 13),
]);

#[rustfmt::skip]
pub(crate) static LUMA_PATTERN: [u16; 64] = vlc_table(&[
    (0x3, 4), (0x5, 5), (0x4, 5), (0x9, 4), (0x3, 5), (0x7, 4), (0x2, 6), (0xb, 4),
    (0x2, 5), (0x3, 6), (0x5, 4), (0xa, 4), (0x4, 4), (0x8, 4), (0x6, 4), (0x3, 2),
]);

#[rustfmt::skip]
pub(crate) static MOTION: [u16; 4096] = vlc_table(&[
    (0x1, 1), (0x1, 2), (0x1, 3), (0x1, 4), (0x3, 6), (0x5, 7), (0x4, 7), (0x3, 7),
    (0xb, 9), (0xa, 9), (0x9, 9), (0x11, 10), (0x10, 10), (0xf, 10), (0xe, 10), (0xd, 10),
    (0xc, 10), (0xb, 10), (0xa, 10), (0x9, 10), (0x8, 10), (0x7, 10), (0x6, 10), (0x5, 10),
    (0x4, 10), (0x7, 11), (0x6, 11), (0x5, 11), (0x4, 11), (0x3, 11), (0x2, 11), (0x3, 12),
    (0x2, 12),
]);

#[rustfmt::skip]
pub(crate) static DC_LUMA: [u16; 2048] = vlc_table(&[
    (0x3, 3), (0x3, 2), (0x2, 2), (0x2, 3), (0x1, 3), (0x1, 4), (0x1, 5), (0x1, 6),
    (0x1, 7), (0x1, 8), (0x1, 9), (0x1, 10), (0x1, 11),
]);

#[rustfmt::skip]
pub(crate) static DC_CHROMA: [u16; 4096] = vlc_table(&[
    (0x3, 2), (0x2, 2), (0x1, 2), (0x1, 3), (0x1, 4), (0x1, 5), (0x1, 6), (0x1, 7),
    (0x1, 8), (0x1, 9), (0x1, 10), (0x1, 11), (0x1, 12),
]);

#[rustfmt::skip]
pub(crate) static INTER: [u16; 4096] = vlc_table(&[
    (0x2, 2), (0xf, 4), (0x15, 6), (0x17, 7), (0x1f, 8), (0x25, 9), (0x24, 9), (0x21, 10),
    (0x20, 10), (0x7, 11), (0x6, 11), (0x20, 11), (0x6, 3), (0x14, 6), (0x1e, 8), (0xf, 10),
    (0x21, 11), (0x50, 12), (0xe, 4), (0x1d, 8), (0xe, 10), (0x51, 12), (0xd, 5), (0x23, 9),
    (0xd, 10), (0xc, 5), (0x22, 9), (0x52, 12), (0xb, 5), (0xc, 10), (0x53, 12), (0x13, 6),
    (0xb, 10), (0x54, 12), (0x12, 6), (0xa, 10), (0x11, 6), (0x9, 10), (0x10, 6), (0x8, 10),
    (0x16, 7), (0x55, 12), (0x15, 7), (0x14, 7), (0x1c, 8), (0x1b, 8), (0x21, 9), (0x20, 9),
    (0x1f, 9), (0x1e, 9), (0x1d, 9), (0x1c, 9), (0x1b, 9), (0x1a, 9), (0x22, 11), (0x23, 11),
    (0x56, 12), (0x57, 12), (0x7, 4), (0x19, 9), (0x5, 11), (0xf, 6), (0x4, 11), (0xe, 6),
    (0xd, 6), (0xc, 6), (0x13, 7), (0x12, 7), (0x11, 7), (0x10, 7), (0x1a, 8), (0x19, 8),
    (0x18, 8), (0x17, 8), (0x16, 8), (0x15, 8), (0x14, 8), (0x13, 8), (0x18, 9), (0x17, 9),
    (0x16, 9), (0x15, 9), (0x14, 9), (0x13, 9), (0x12, 9), (0x11, 9), (0x7, 10), (0x6, 10),
    (0x5, 10), (0x4, 10), (0x24, 11), (0x25, 11), (0x26, 11), (0x27, 11), (0x58, 12), (0x59, 12),
    (0x5a, 12), (0x5b, 12), (0x5c, 12), (0x5d, 12), (0x5e, 12), (0x5f, 12), (0x3, 7),
]);

#[rustfmt::skip]
pub(crate) static INTRA: [u16; 4096] = vlc_table(&[
    (0x2, 2), (0x6, 3), (0xf, 4), (0xd, 5), (0xc, 5), (0x15, 6), (0x13, 6), (0x12, 6),
    (0x17, 7), (0x1f, 8), (0x1e, 8), (0x1d, 8), (0x25, 9), (0x24, 9), (0x23, 9), (0x21, 9),
    (0x21, 10), (0x20, 10), (0xf, 10), (0xe, 10), (0x7, 11), (0x6, 11), (0x20, 11), (0x21, 11),
    (0x50, 12), (0x51, 12), (0x52, 12), (0xe, 4), (0x14, 6), (0x16, 7), (0x1c, 8), (0x20, 9),
    (0x1f, 9), (0xd, 10), (0x22, 11), (0x53, 12), (0x55, 12), (0xb, 5), (0x15, 7), (0x1e, 9),
    (0xc, 10), (0x56, 12), (0x11, 6), (0x1b, 8), (0x1d, 9), (0xb, 10), (0x10, 6), (0x22, 9),
    (0xa, 10), (0xd, 6), (0x1c, 9), (0x8, 10), (0x12, 7), (0x1b, 9), (0x54, 12), (0x14, 7),
    (0x1a, 9), (0x57, 12), (0x19, 8), (0x9, 10), (0x18, 8), (0x23, 11), (0x17, 8), (0x19, 9),
    (0x18, 9), (0x7, 10), (0x58, 12), (0x7, 4), (0xc, 6), (0x16, 8), (0x17, 9), (0x6, 10),
    (0x5, 11), (0x4, 11), (0x59, 12), (0xf, 6), (0x16, 9), (0x5, 10), (0xe, 6), (0x4, 10),
    (0x11, 7), (0x24, 11), (0x10, 7), (0x25, 11), (0x13, 7), (0x5a, 12), (0x15, 8), (0x5b, 12),
    (0x14, 8), (0x13, 8), (0x1a, 8), (0x15, 9), (0x14, 9), (0x13, 9), (0x12, 9), (0x11, 9),
    (0x26, 11), (0x27, 11), (0x5c, 12), (0x5d, 12), (0x5e, 12), (0x5f, 12), (0x3, 7),
]);

#[rustfmt::skip]
pub(crate) const INTER_VALUES: [(usize, i32, bool); 102] = [
    (0, 1, false), (0, 2, false), (0, 3, false), (0, 4, false), (0, 5, false), (0, 6, false),
    (0, 7, false), (0, 8, false), (0, 9, false), (0, 10, false), (0, 11, false), (0, 12, false),
    (1, 1, false), (1, 2, false), (1, 3, false), (1, 4, false), (1, 5, false), (1, 6, false),
    (2, 1, false), (2, 2, false), (2, 3, false), (2, 4, false), (3, 1, false), (3, 2, false),
    (3, 3, false), (4, 1, false), (4, 2, false), (4, 3, false), (5, 1, false), (5, 2, false),
    (5, 3, false), (6, 1, false), (6, 2, false), (6, 3, false), (7, 1, false), (7, 2, false),
    (8, 1, false), (8, 2, false), (9, 1, false), (9, 2, false), (10, 1, false), (10, 2, false),
    (11, 1, false), (12, 1, false), (13, 1, false), (14, 1, false), (15, 1, false), (16, 1, false),
    (17, 1, false), (18, 1, false), (19, 1, false), (20, 1, false), (21, 1, false), (22, 1, false),
    (23, 1, false), (24, 1, false), (25, 1, false), (26, 1, false), (0, 1, true), (0, 2, true),
    (0, 3, true), (1, 1, true), (1, 2, true), (2, 1, true), (3, 1, true), (4, 1, true),
    (5, 1, true), (6, 1, true), (7, 1, true), (8, 1, true), (9, 1, true), (10, 1, true),
    (11, 1, true), (12, 1, true), (13, 1, true), (14, 1, true), (15, 1, true), (16, 1, true),
    (17, 1, true), (18, 1, true), (19, 1, true), (20, 1, true), (21, 1, true), (22, 1, true),
    (23, 1, true), (24, 1, true), (25, 1, true), (26, 1, true), (27, 1, true), (28, 1, true),
    (29, 1, true), (30, 1, true), (31, 1, true), (32, 1, true), (33, 1, true), (34, 1, true),
    (35, 1, true), (36, 1, true), (37, 1, true), (38, 1, true), (39, 1, true), (40, 1, true),
];

#[rustfmt::skip]
pub(crate) const INTRA_VALUES: [(usize, i32, bool); 102] = [
    (0, 1, false), (0, 2, false), (0, 3, false), (0, 4, false), (0, 5, false), (0, 6, false),
    (0, 7, false), (0, 8, false), (0, 9, false), (0, 10, false), (0, 11, false), (0, 12, false),
    (0, 13, false), (0, 14, false), (0, 15, false), (0, 16, false), (0, 17, false), (0, 18, false),
    (0, 19, false), (0, 20, false), (0, 21, false), (0, 22, false), (0, 23, false), (0, 24, false),
    (0, 25, false), (0, 26, false), (0, 27, false), (1, 1, false), (1, 2, false), (1, 3, false),
    (1, 4, false), (1, 5, false), (1, 6, false), (1, 7, false), (1, 8, false), (1, 9, false),
    (1, 10, false), (2, 1, false), (2, 2, false), (2, 3, false), (2, 4, false), (2, 5, false),
    (3, 1, false), (3, 2, false), (3, 3, false), (3, 4, false), (4, 1, false), (4, 2, false),
    (4, 3, false), (5, 1, false), (5, 2, false), (5, 3, false), (6, 1, false), (6, 2, false),
    (6, 3, false), (7, 1, false), (7, 2, false), (7, 3, false), (8, 1, false), (8, 2, false),
    (9, 1, false), (9, 2, false), (10, 1, false), (11, 1, false), (12, 1, false), (13, 1, false),
    (14, 1, false), (0, 1, true), (0, 2, true), (0, 3, true), (0, 4, true), (0, 5, true),
    (0, 6, true), (0, 7, true), (0, 8, true), (1, 1, true), (1, 2, true), (1, 3, true),
    (2, 1, true), (2, 2, true), (3, 1, true), (3, 2, true), (4, 1, true), (4, 2, true),
    (5, 1, true), (5, 2, true), (6, 1, true), (6, 2, true), (7, 1, true), (8, 1, true),
    (9, 1, true), (10, 1, true), (11, 1, true), (12, 1, true), (13, 1, true), (14, 1, true),
    (15, 1, true), (16, 1, true), (17, 1, true), (18, 1, true), (19, 1, true), (20, 1, true),
];

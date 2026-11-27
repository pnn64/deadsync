//! Constant tables used for motion vector prediction (spec §6.5 "Motion vector prediction").
//! Split out of `prob_tables.rs` (W5) since these are specific to `tile::mv_pred`'s
//! `find_mv_refs`, not general syntax-element probability/tree tables. Doc-hidden internal
//! module, same convention as the rest of `src/` (see `lib.rs`).

/// `mv_ref_blocks[ BLOCK_SIZES ][ MVREF_NEIGHBOURS ][ 2 ]` (spec §6.5.1).
/// Each element is `(deltaRow, deltaCol)`.
pub const MV_REF_BLOCKS: [[(i32, i32); 8]; 13] = [
    [
        (-1, 0),
        (0, -1),
        (-1, -1),
        (-2, 0),
        (0, -2),
        (-2, -1),
        (-1, -2),
        (-2, -2),
    ],
    [
        (-1, 0),
        (0, -1),
        (-1, -1),
        (-2, 0),
        (0, -2),
        (-2, -1),
        (-1, -2),
        (-2, -2),
    ],
    [
        (-1, 0),
        (0, -1),
        (-1, -1),
        (-2, 0),
        (0, -2),
        (-2, -1),
        (-1, -2),
        (-2, -2),
    ],
    [
        (-1, 0),
        (0, -1),
        (-1, -1),
        (-2, 0),
        (0, -2),
        (-2, -1),
        (-1, -2),
        (-2, -2),
    ],
    [
        (0, -1),
        (-1, 0),
        (1, -1),
        (-1, -1),
        (0, -2),
        (-2, 0),
        (-2, -1),
        (-1, -2),
    ],
    [
        (-1, 0),
        (0, -1),
        (-1, 1),
        (-1, -1),
        (-2, 0),
        (0, -2),
        (-1, -2),
        (-2, -1),
    ],
    [
        (-1, 0),
        (0, -1),
        (-1, 1),
        (1, -1),
        (-1, -1),
        (-3, 0),
        (0, -3),
        (-3, -3),
    ],
    [
        (0, -1),
        (-1, 0),
        (2, -1),
        (-1, -1),
        (-1, 1),
        (0, -3),
        (-3, 0),
        (-3, -3),
    ],
    [
        (-1, 0),
        (0, -1),
        (-1, 2),
        (-1, -1),
        (1, -1),
        (-3, 0),
        (0, -3),
        (-3, -3),
    ],
    [
        (-1, 1),
        (1, -1),
        (-1, 2),
        (2, -1),
        (-1, -1),
        (-3, 0),
        (0, -3),
        (-3, -3),
    ],
    [
        (0, -1),
        (-1, 0),
        (4, -1),
        (-1, 2),
        (-1, -1),
        (0, -3),
        (-3, 0),
        (2, -1),
    ],
    [
        (-1, 0),
        (0, -1),
        (-1, 4),
        (2, -1),
        (-1, -1),
        (-3, 0),
        (0, -3),
        (-1, 2),
    ],
    [
        (-1, 3),
        (3, -1),
        (-1, 4),
        (4, -1),
        (-1, -1),
        (-1, 0),
        (0, -1),
        (-1, 6),
    ],
];

/// `mode_2_counter[ MB_MODE_COUNT ]` (spec §6.5.1). Maps `YModes` values (intra 0..9,
/// inter `NEARESTMV`..`NEWMV` 10..13) to the value added to `contextCounter`.
pub const MODE_2_COUNTER: [u8; 14] = [9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 0, 0, 3, 1];

/// `counter_to_context[ 19 ]` (spec §6.5.1). Converts `contextCounter` (0..=18) to the
/// `inter_mode_probs` context (0..=6; values above 7 are `INVALID_CASE` and never
/// actually occur).
pub const COUNTER_TO_CONTEXT: [u8; 19] = [2, 3, 4, 1, 3, 9, 0, 9, 9, 5, 5, 9, 5, 9, 9, 9, 9, 9, 6];

/// `idx_n_column_to_subblock[ 4 ][ 2 ]` (spec §6.5.11). Used by `get_sub_block_mv`.
pub const IDX_N_COLUMN_TO_SUBBLOCK: [[u8; 2]; 4] = [[1, 2], [1, 3], [3, 2], [3, 3]];

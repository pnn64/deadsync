//! Counter collection and `merge_prob`/`merge_probs`/`adapt_coef_probs`/
//! `adapt_noncoef_probs` for probability adaptation (backward adaptation, spec §8.4).
//!
//! The shape and update conditions of the counters strictly follow spec §8.3
//! (`clear_counts`, each array's dimensions) and §9.3.4 ("Syntax element
//! counting process", the table mapping which syntax element updates which
//! counter). Each syntax-element read site in `src/tile.rs` increments the
//! fields of this struct.

use crate::compressed_header::CompressedHeaderProbs;
use crate::prob_tables::{
    CoefProbs, BINARY_TREE, COUNT_SAT, INTERP_FILTER_TREE, INTER_MODE_TREE, INTRA_MODE_TREE,
    MAX_UPDATE_FACTOR, MV_CLASS_TREE, MV_FR_TREE, MV_JOINT_TREE, PARTITION_TREE, SMALL_TOKEN_TREE,
    SWITCHABLE, TX_16X16, TX_32X32, TX_8X8, TX_MODE_SELECT,
};

/// `counts_token[TX_SIZES][BLOCK_TYPES][REF_TYPES][COEF_BANDS][PREV_COEF_CONTEXTS][UNCONSTRAINED_NODES]`.
pub type TokenCounts = [[[[[[u32; 3]; 6]; 6]; 2]; 2]; 4];
/// `counts_more_coefs[TX_SIZES][BLOCK_TYPES][REF_TYPES][COEF_BANDS][PREV_COEF_CONTEXTS][2]`.
pub type MoreCoefsCounts = [[[[[[u32; 2]; 6]; 6]; 2]; 2]; 4];

/// All counter arrays enumerated by the "Clear counts process" in spec §8.3.
#[derive(Debug, Clone, Default)]
pub struct Counts {
    pub intra_mode: [[u32; 10]; 4],
    pub uv_mode: [[u32; 10]; 10],
    pub partition: [[u32; 4]; 16],
    pub interp_filter: [[u32; 3]; 4],
    pub inter_mode: [[u32; 4]; 7],
    /// `counts_tx_size[TX_SIZES][TX_SIZE_CONTEXTS][TX_SIZES]`. `tx_size[maxTxSize][ctx][value]`.
    pub tx_size: [[[u32; 4]; 2]; 4],
    pub is_inter: [[u32; 2]; 4],
    pub comp_mode: [[u32; 2]; 5],
    /// `counts_single_ref[REF_CONTEXTS][2][2]`. `single_ref[ctx][p1_or_p2][value]`.
    pub single_ref: [[[u32; 2]; 2]; 5],
    pub comp_ref: [[u32; 2]; 5],
    pub skip: [[u32; 2]; 3],
    pub mv_joint: [u32; 4],
    pub mv_sign: [[u32; 2]; 2],
    pub mv_class: [[u32; 11]; 2],
    pub mv_class0_bit: [[u32; 2]; 2],
    pub mv_class0_fr: [[[u32; 4]; 2]; 2],
    pub mv_class0_hp: [[u32; 2]; 2],
    pub mv_bits: [[[u32; 2]; 10]; 2],
    pub mv_fr: [[u32; 4]; 2],
    pub mv_hp: [[u32; 2]; 2],
    pub token: TokenCounts,
    pub more_coefs: MoreCoefsCounts,
}

impl Counts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resets every counter back to zero in place for cross-frame `Box<Counts>` reuse.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// `merge_prob( preProb, ct0, ct1, countSat, maxUpdateFactor )` (spec §8.4.1).
fn merge_prob(pre_prob: u8, ct0: u32, ct1: u32, count_sat: u32, max_update_factor: u32) -> u8 {
    let den = ct0 + ct1;
    let prob = if den == 0 {
        128
    } else {
        (((ct0 as u64 * 256 + (den as u64 >> 1)) / den as u64) as i64).clamp(1, 255) as u8
    };
    let count = den.min(count_sat);
    let factor = max_update_factor * count / count_sat;
    let out = (pre_prob as i64 * (256 - factor as i64) + prob as i64 * factor as i64 + 128) >> 8;
    out as u8
}

/// `merge_probs( tree, i, probs, counts, countSat, maxUpdateFactor )` (spec §8.4.2).
/// Returns `leftCount + rightCount`.
fn merge_probs(
    tree: &[i32],
    i: usize,
    probs: &mut [u8],
    counts: &[u32],
    count_sat: u32,
    max_update_factor: u32,
) -> u32 {
    let s = tree[i];
    let left_count = if s <= 0 {
        counts[(-s) as usize]
    } else {
        merge_probs(
            tree,
            s as usize,
            probs,
            counts,
            count_sat,
            max_update_factor,
        )
    };
    let r = tree[i + 1];
    let right_count = if r <= 0 {
        counts[(-r) as usize]
    } else {
        merge_probs(
            tree,
            r as usize,
            probs,
            counts,
            count_sat,
            max_update_factor,
        )
    };
    probs[i >> 1] = merge_prob(
        probs[i >> 1],
        left_count,
        right_count,
        count_sat,
        max_update_factor,
    );
    left_count + right_count
}

/// `adapt_probs( tree, probs, counts )` (spec §8.4.4).
fn adapt_probs(tree: &[i32], probs: &mut [u8], counts: &[u32]) {
    merge_probs(tree, 0, probs, counts, COUNT_SAT, MAX_UPDATE_FACTOR);
}

/// `adapt_prob( prob, counts )` (spec §8.4.4).
fn adapt_prob(prob: u8, counts: [u32; 2]) -> u8 {
    merge_prob(prob, counts[0], counts[1], COUNT_SAT, MAX_UPDATE_FACTOR)
}

/// `adapt_coef_probs( )` (spec §8.4.3). `update_factor` is determined by the
/// caller (based on `FrameIsIntra`/`LastFrameType`; see the spec text) and passed in.
///
/// `t`/`i`/`j`/`k`/`l` walk `coef_probs`/`counts.token`/`counts.more_coefs`
/// simultaneously with the same indices, so rewriting this with
/// `iter_mut().enumerate()` would only make it harder to read (this is a
/// direct transcription of the spec's pseudocode `for`-loop structure into a
/// Rust indexed loop).
#[allow(clippy::needless_range_loop)]
pub fn adapt_coef_probs(coef_probs: &mut CoefProbs, counts: &Counts, update_factor: u32) {
    for t in 0..4 {
        for i in 0..2 {
            for j in 0..2 {
                for k in 0..6 {
                    let max_l = if k == 0 { 3 } else { 6 };
                    for l in 0..max_l {
                        let probs = &mut coef_probs[t][i][j][k][l];
                        merge_probs(
                            &SMALL_TOKEN_TREE,
                            2,
                            probs,
                            &counts.token[t][i][j][k][l],
                            24,
                            update_factor,
                        );
                        merge_probs(
                            &BINARY_TREE,
                            0,
                            probs,
                            &counts.more_coefs[t][i][j][k][l],
                            24,
                            update_factor,
                        );
                    }
                }
            }
        }
    }
}

fn adapt_prob_slice(probs: &mut [u8], counts: &[[u32; 2]]) {
    for (p, &c) in probs.iter_mut().zip(counts) {
        *p = adapt_prob(*p, c);
    }
}

fn adapt_probs_rows<const P: usize, const C: usize>(
    tree: &[i32],
    probs: &mut [[u8; P]],
    counts: &[[u32; C]],
) {
    for (p, c) in probs.iter_mut().zip(counts) {
        adapt_probs(tree, p, c);
    }
}

/// `adapt_noncoef_probs( )` (spec §8.4.4).
pub fn adapt_noncoef_probs(
    probs: &mut CompressedHeaderProbs,
    counts: &Counts,
    interpolation_filter: u8,
    tx_mode: u8,
    allow_high_precision_mv: bool,
) {
    adapt_prob_slice(&mut probs.is_inter_prob, &counts.is_inter);
    adapt_prob_slice(&mut probs.comp_mode_prob, &counts.comp_mode);
    adapt_prob_slice(&mut probs.comp_ref_prob, &counts.comp_ref);
    adapt_prob_slice(
        probs.single_ref_prob.as_flattened_mut(),
        counts.single_ref.as_flattened(),
    );
    adapt_probs_rows(
        &INTER_MODE_TREE,
        &mut probs.inter_mode_probs,
        &counts.inter_mode,
    );
    adapt_probs_rows(
        &INTRA_MODE_TREE,
        &mut probs.y_mode_probs,
        &counts.intra_mode,
    );
    adapt_probs_rows(&INTRA_MODE_TREE, &mut probs.uv_mode_probs, &counts.uv_mode);
    adapt_probs_rows(
        &PARTITION_TREE,
        &mut probs.partition_probs,
        &counts.partition,
    );
    adapt_prob_slice(&mut probs.skip_prob, &counts.skip);
    if interpolation_filter == SWITCHABLE {
        adapt_probs_rows(
            &INTERP_FILTER_TREE,
            &mut probs.interp_filter_probs,
            &counts.interp_filter,
        );
    }
    if tx_mode == TX_MODE_SELECT {
        for (tx_sz, tree) in [
            (TX_8X8, crate::prob_tables::TX_SIZE_8_TREE.as_slice()),
            (TX_16X16, crate::prob_tables::TX_SIZE_16_TREE.as_slice()),
            (TX_32X32, crate::prob_tables::TX_SIZE_32_TREE.as_slice()),
        ] {
            adapt_probs_rows(
                tree,
                &mut probs.tx_probs[tx_sz as usize],
                &counts.tx_size[tx_sz as usize],
            );
        }
    }
    adapt_probs(&MV_JOINT_TREE, &mut probs.mv_joint_probs, &counts.mv_joint);
    for i in 0..2 {
        probs.mv_sign_prob[i] = adapt_prob(probs.mv_sign_prob[i], counts.mv_sign[i]);
        adapt_probs(
            &MV_CLASS_TREE,
            &mut probs.mv_class_probs[i],
            &counts.mv_class[i],
        );
        probs.mv_class0_bit_prob[i] =
            adapt_prob(probs.mv_class0_bit_prob[i], counts.mv_class0_bit[i]);
        adapt_prob_slice(&mut probs.mv_bits_prob[i], &counts.mv_bits[i]);
        adapt_probs_rows(
            &MV_FR_TREE,
            &mut probs.mv_class0_fr_probs[i],
            &counts.mv_class0_fr[i],
        );
        adapt_probs(&MV_FR_TREE, &mut probs.mv_fr_probs[i], &counts.mv_fr[i]);
        if allow_high_precision_mv {
            probs.mv_class0_hp_prob[i] =
                adapt_prob(probs.mv_class0_hp_prob[i], counts.mv_class0_hp[i]);
            probs.mv_hp_prob[i] = adapt_prob(probs.mv_hp_prob[i], counts.mv_hp[i]);
        }
    }
}

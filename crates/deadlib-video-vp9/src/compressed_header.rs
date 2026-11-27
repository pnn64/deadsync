//! Parsing of the compressed header (`compressed_header`) (spec §6.3).
//!
//! `compressed_header()` is `header_size_in_bytes` bytes of bool-coded data
//! holding the transform mode (`tx_mode`) and the update contents of the
//! various probability tables.
//!
//! ```text
//! compressed_header( ) {
//!     read_tx_mode( )
//!     if ( tx_mode == TX_MODE_SELECT ) {
//!        tx_mode_probs( )
//!     }
//!     read_coef_probs( )
//!     read_skip_prob( )
//!     if ( FrameIsIntra == 0 ) {
//!        read_inter_mode_probs( )
//!        if ( interpolation_filter == SWITCHABLE ) read_interp_filter_probs( )
//!        read_is_inter_probs( )
//!        frame_reference_mode( )
//!        frame_reference_mode_probs( )
//!        read_y_mode_probs( )
//!        read_partition_probs( )
//!        mv_probs( )
//!     }
//! }
//! ```
//!
//! The inter-related reads called only when `FrameIsIntra == 0`
//! (`read_inter_mode_probs` onward, spec §6.3.9-6.3.18) were implemented in M3.

use core::fmt;

use crate::bool_coder::{BoolCoderError, BoolDecoder};
use crate::header::NewFrameHeader;
use crate::prob_tables::{
    CoefProbs, ALTREF_FRAME, COMPOUND_REFERENCE, DEFAULT_COEF_PROBS, DEFAULT_COMP_MODE_PROB,
    DEFAULT_COMP_REF_PROB, DEFAULT_INTERP_FILTER_PROBS, DEFAULT_INTER_MODE_PROBS,
    DEFAULT_IS_INTER_PROB, DEFAULT_MV_BITS_PROB, DEFAULT_MV_CLASS0_BIT_PROB,
    DEFAULT_MV_CLASS0_FR_PROBS, DEFAULT_MV_CLASS0_HP_PROB, DEFAULT_MV_CLASS_PROBS,
    DEFAULT_MV_FR_PROBS, DEFAULT_MV_HP_PROB, DEFAULT_MV_JOINT_PROBS, DEFAULT_MV_SIGN_PROB,
    DEFAULT_PARTITION_PROBS, DEFAULT_SINGLE_REF_PROB, DEFAULT_SKIP_PROB, DEFAULT_TX_PROBS,
    DEFAULT_UV_MODE_PROBS, DEFAULT_Y_MODE_PROBS, GOLDEN_FRAME, INV_MAP_TABLE, LAST_FRAME,
    REFERENCE_MODE_SELECT, SINGLE_REFERENCE, SWITCHABLE, TX_16X16, TX_32X32, TX_4X4, TX_8X8,
    TX_MODE_SELECT, TX_MODE_TO_BIGGEST_TX_SIZE,
};

/// Errors that can occur while parsing `compressed_header`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressedHeaderError {
    /// Bool decoder initialization failed (e.g. `header_size_in_bytes` is 0).
    BoolCoder(BoolCoderError),
}

impl From<BoolCoderError> for CompressedHeaderError {
    fn from(e: BoolCoderError) -> Self {
        CompressedHeaderError::BoolCoder(e)
    }
}

impl fmt::Display for CompressedHeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompressedHeaderError::BoolCoder(e) => {
                write!(f, "compressed header bool decoder error: {e}")
            }
        }
    }
}

impl core::error::Error for CompressedHeaderError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            CompressedHeaderError::BoolCoder(e) => Some(e),
        }
    }
}

/// The full set of probability tables updated by `compressed_header()`.
///
/// Corresponds to "all probability tables" operated on by the spec's
/// `load_probs`/`save_probs` (spec §7.1.2), and is saved/restored as-is as the
/// frame context ([`FrameContext`], 4 slots).
///
/// `uv_mode_probs` has no forward update syntax in `compressed_header()`
/// (`read_y_mode_probs()` only updates `y_mode_probs`), but it is a target of
/// backward adaptation in spec §8.4.4 `adapt_noncoef_probs()`
/// (`adapt_probs( intra_mode_tree, uv_mode_probs[ i ], counts_uv_mode[ i ] )`),
/// so it is kept here as one of the tables operated on by `load_probs`/`save_probs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressedHeaderProbs {
    /// `uv_mode_probs[y_mode][node]`. Has no forward update syntax but is a
    /// target of backward adaptation (see doc comment above).
    pub uv_mode_probs: [[u8; 9]; 10],
    /// `tx_probs[maxTxSize][ctx][node]`. Same layout as [`crate::prob_tables::DEFAULT_TX_PROBS`].
    pub tx_probs: [[[u8; 3]; 2]; 4],
    /// `coef_probs[txSz][plane>0][is_inter][band][ctx][node]`.
    pub coef_probs: CoefProbs,
    /// `skip_prob[ctx]` (spec §6.3.8).
    pub skip_prob: [u8; 3],
    /// `inter_mode_probs[ctx][node]` (spec §6.3.9). Updated only when `FrameIsIntra == 0`.
    pub inter_mode_probs: [[u8; 3]; 7],
    /// `interp_filter_probs[ctx][node]` (spec §6.3.10).
    pub interp_filter_probs: [[u8; 2]; 4],
    /// `is_inter_prob[ctx]` (spec §6.3.11).
    pub is_inter_prob: [u8; 4],
    /// `comp_mode_prob[ctx]` (spec §6.3.13).
    pub comp_mode_prob: [u8; 5],
    /// `single_ref_prob[ctx][0..2]` (spec §6.3.13).
    pub single_ref_prob: [[u8; 2]; 5],
    /// `comp_ref_prob[ctx]` (spec §6.3.13).
    pub comp_ref_prob: [u8; 5],
    /// `y_mode_probs[ctx][node]` (spec §6.3.14). Non-key-frame only (key
    /// frames always use the fixed table [`crate::prob_tables::KF_Y_MODE_PROBS`]).
    pub y_mode_probs: [[u8; 9]; 4],
    /// `partition_probs[ctx][node]` (spec §6.3.15). Non-key-frame only (key
    /// frames always use the fixed table [`crate::prob_tables::KF_PARTITION_PROBS`]).
    pub partition_probs: [[u8; 3]; 16],
    /// `mv_joint_probs[node]` (spec §6.3.16).
    pub mv_joint_probs: [u8; 3],
    /// `mv_sign_prob[comp]`.
    pub mv_sign_prob: [u8; 2],
    /// `mv_class_probs[comp][node]`.
    pub mv_class_probs: [[u8; 10]; 2],
    /// `mv_class0_bit_prob[comp]`.
    pub mv_class0_bit_prob: [u8; 2],
    /// `mv_bits_prob[comp][i]`.
    pub mv_bits_prob: [[u8; 10]; 2],
    /// `mv_class0_fr_probs[comp][class0bit][node]`.
    pub mv_class0_fr_probs: [[[u8; 3]; 2]; 2],
    /// `mv_fr_probs[comp][node]`.
    pub mv_fr_probs: [[u8; 3]; 2],
    /// `mv_class0_hp_prob[comp]`.
    pub mv_class0_hp_prob: [u8; 2],
    /// `mv_hp_prob[comp]`.
    pub mv_hp_prob: [u8; 2],
}

impl Default for CompressedHeaderProbs {
    fn default() -> Self {
        Self {
            uv_mode_probs: DEFAULT_UV_MODE_PROBS,
            tx_probs: DEFAULT_TX_PROBS,
            coef_probs: DEFAULT_COEF_PROBS,
            skip_prob: DEFAULT_SKIP_PROB,
            inter_mode_probs: DEFAULT_INTER_MODE_PROBS,
            interp_filter_probs: DEFAULT_INTERP_FILTER_PROBS,
            is_inter_prob: DEFAULT_IS_INTER_PROB,
            comp_mode_prob: DEFAULT_COMP_MODE_PROB,
            single_ref_prob: DEFAULT_SINGLE_REF_PROB,
            comp_ref_prob: DEFAULT_COMP_REF_PROB,
            y_mode_probs: DEFAULT_Y_MODE_PROBS,
            partition_probs: DEFAULT_PARTITION_PROBS,
            mv_joint_probs: DEFAULT_MV_JOINT_PROBS,
            mv_sign_prob: DEFAULT_MV_SIGN_PROB,
            mv_class_probs: DEFAULT_MV_CLASS_PROBS,
            mv_class0_bit_prob: DEFAULT_MV_CLASS0_BIT_PROB,
            mv_bits_prob: DEFAULT_MV_BITS_PROB,
            mv_class0_fr_probs: DEFAULT_MV_CLASS0_FR_PROBS,
            mv_fr_probs: DEFAULT_MV_FR_PROBS,
            mv_class0_hp_prob: DEFAULT_MV_CLASS0_HP_PROB,
            mv_hp_prob: DEFAULT_MV_HP_PROB,
        }
    }
}

/// Frame context (the unit operated on by `load_probs`/`save_probs` in spec §7.1.2).
/// `CompressedHeaderProbs` itself corresponds to "all probability tables that are saved and restored".
pub type FrameContext = CompressedHeaderProbs;

/// The 4-slot frame context storage area, addressed by `frame_context_idx` (0..=3).
///
/// Spec §7.2's `setup_past_independence()` resets all probability tables to
/// their default values for key frames, intra-only frames, and error-resilient
/// frames, and then (under conditions such as `frame_type == KEY_FRAME`) calls
/// `save_probs(i)` for all 4 slots. For non-key frames, `load_probs` is
/// performed from the slot pointed to by `frame_context_idx` (the
/// `starting_probs` argument of `parse_compressed_header` in this decoder),
/// and if `refresh_frame_context == 1`, the result is written back to the same
/// slot via `save_probs`.
///
/// Backward probability adaptation from spec §8.4
/// (`adapt_coef_probs`/`adapt_noncoef_probs`, based on observed frequencies)
/// is implemented in `counts.rs` and driven by `Decoder` (see
/// `refresh_probs` in `lib.rs`), which adapts the probabilities after tile
/// decode and writes them back via `save_probs` when
/// `refresh_frame_context == 1`.
#[derive(Debug, Clone)]
pub struct FrameContextStore {
    contexts: [FrameContext; 4],
}

impl FrameContextStore {
    /// Initializes all 4 slots with default values (equivalent to calling
    /// `save_probs(i)` for i in 0..4 right after `setup_past_independence()`).
    pub fn new() -> Self {
        Self {
            contexts: core::array::from_fn(|_| FrameContext::default()),
        }
    }

    pub fn load(&self, idx: u8) -> FrameContext {
        self.contexts[idx as usize].clone()
    }

    pub fn save(&mut self, idx: u8, ctx: FrameContext) {
        self.contexts[idx as usize] = ctx;
    }

    /// The all-slots reset performed by `setup_past_independence()` (occurs
    /// for key frames, error-resilient frames, etc.).
    pub fn reset_all(&mut self) {
        self.contexts = core::array::from_fn(|_| FrameContext::default());
    }
}

impl Default for FrameContextStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of parsing `compressed_header()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressedHeader {
    /// `tx_mode` (spec §7.3.1).
    pub tx_mode: u8,
    /// Updated probability tables borrowed by the tile decoder.
    pub probs: CompressedHeaderProbs,
    /// `reference_mode` (spec §7.3.6). Always `SINGLE_REFERENCE` when `FrameIsIntra == 1`.
    pub reference_mode: u8,
    /// `CompFixedRef` (spec §6.3.18). Unused (0) when `reference_mode == SINGLE_REFERENCE`.
    pub comp_fixed_ref: u8,
    /// `CompVarRef[ 0..2 ]` (spec §6.3.18). Unused (`[0, 0]`) when
    /// `reference_mode == SINGLE_REFERENCE`.
    pub comp_var_ref: [u8; 2],
}

/// A `B(252)` bool read plus update decision, equivalent to `read_prob()` (spec §6.3.3 `diff_update_prob`).
///
/// ```text
/// diff_update_prob( prob ) {
///     update_prob                  B(252)
///     if ( update_prob == 1 ) {
///        deltaProb = decode_term_subexp( )
///        prob = inv_remap_prob( deltaProb, prob )
///     }
///     return prob
/// }
/// ```
fn diff_update_prob(r: &mut BoolDecoder, prob: u8) -> u8 {
    let update_prob = r.read_bool(252);
    if update_prob {
        let delta_prob = decode_term_subexp(r);
        inv_remap_prob(delta_prob, prob)
    } else {
        prob
    }
}

/// `decode_term_subexp()` (spec §6.3.4). All fields are read via `L(n)` (`read_literal`).
fn decode_term_subexp(r: &mut BoolDecoder) -> u32 {
    if !r.flag() {
        return r.read_literal(4);
    }
    if !r.flag() {
        return r.read_literal(4) + 16;
    }
    if !r.flag() {
        return r.read_literal(5) + 32;
    }
    let v = r.read_literal(7);
    if v < 65 {
        return v + 64;
    }
    let bit = r.read_literal(1);
    (v << 1) - 1 + bit
}

/// `inv_remap_prob( deltaProb, prob )` (spec §6.3.5).
fn inv_remap_prob(delta_prob: u32, prob: u8) -> u8 {
    let v = INV_MAP_TABLE[delta_prob as usize] as u32;
    // m-- (prob minus 1) is used from here on.
    let m = prob as i32 - 1;
    let result = if (m << 1) <= 255 {
        1 + inv_recenter_nonneg(v, m as u32) as i32
    } else {
        255 - inv_recenter_nonneg(v, (255 - 1 - m) as u32) as i32
    };
    result as u8
}

/// `inv_recenter_nonneg( v, m )` (spec §6.3.6).
fn inv_recenter_nonneg(v: u32, m: u32) -> u32 {
    if v > 2 * m {
        return v;
    }
    if v & 1 == 1 {
        m - ((v + 1) >> 1)
    } else {
        m + (v >> 1)
    }
}

/// `read_tx_mode()` (spec §6.3.1).
fn read_tx_mode(r: &mut BoolDecoder, lossless: bool) -> u8 {
    if lossless {
        TX_4X4 // Same value as ONLY_4X4 (0)
    } else {
        let mut tx_mode = r.read_literal(2) as u8;
        if tx_mode == TX_32X32 {
            // ALLOW_32X32 (=3) and TX_32X32 (=3) share the same value, so the same constant is reused.
            let tx_mode_select = r.read_literal(1) as u8;
            tx_mode += tx_mode_select;
        }
        tx_mode
    }
}

/// `tx_mode_probs()` (spec §6.3.2). Called only when `tx_mode == TX_MODE_SELECT`.
fn read_tx_mode_probs(r: &mut BoolDecoder, tx_probs: &mut [[[u8; 3]; 2]; 4]) {
    for (tx_sz, nodes) in [(TX_8X8, 1), (TX_16X16, 2), (TX_32X32, 3)] {
        for ctx in tx_probs[tx_sz as usize].iter_mut() {
            diff_update_prob_slice(r, &mut ctx[..nodes]);
        }
    }
}

/// `read_coef_probs()` (spec §6.3.7).
fn read_coef_probs(r: &mut BoolDecoder, tx_mode: u8, coef_probs: &mut CoefProbs) {
    let max_tx_size = TX_MODE_TO_BIGGEST_TX_SIZE[tx_mode as usize];
    for tx_sz in 0..=max_tx_size {
        if !r.flag() {
            continue;
        }
        for plane_probs in coef_probs[tx_sz as usize].iter_mut() {
            for ref_probs in plane_probs.iter_mut() {
                for (k, band_probs) in ref_probs.iter_mut().enumerate() {
                    let max_l = if k == 0 { 3 } else { 6 };
                    for ctx_probs in band_probs.iter_mut().take(max_l) {
                        diff_update_prob_slice(r, ctx_probs);
                    }
                }
            }
        }
    }
}

/// Applies [`diff_update_prob`] to every probability in `probs` in place.
fn diff_update_prob_slice(r: &mut BoolDecoder, probs: &mut [u8]) {
    for prob in probs.iter_mut() {
        *prob = diff_update_prob(r, *prob);
    }
}

/// `setup_compound_reference_mode()` (spec §6.3.18). Returns `(CompFixedRef, CompVarRef)`.
fn setup_compound_reference_mode(ref_frame_sign_bias: &[bool; 4]) -> (u8, [u8; 2]) {
    if ref_frame_sign_bias[LAST_FRAME as usize] == ref_frame_sign_bias[GOLDEN_FRAME as usize] {
        (ALTREF_FRAME, [LAST_FRAME, GOLDEN_FRAME])
    } else if ref_frame_sign_bias[LAST_FRAME as usize] == ref_frame_sign_bias[ALTREF_FRAME as usize]
    {
        (GOLDEN_FRAME, [LAST_FRAME, ALTREF_FRAME])
    } else {
        (LAST_FRAME, [GOLDEN_FRAME, ALTREF_FRAME])
    }
}

/// `frame_reference_mode()` (spec §6.3.12). Returns
/// `(reference_mode, CompFixedRef, CompVarRef)`.
fn frame_reference_mode(r: &mut BoolDecoder, ref_frame_sign_bias: &[bool; 4]) -> (u8, u8, [u8; 2]) {
    let compound_reference_allowed = ref_frame_sign_bias[GOLDEN_FRAME as usize]
        != ref_frame_sign_bias[LAST_FRAME as usize]
        || ref_frame_sign_bias[ALTREF_FRAME as usize] != ref_frame_sign_bias[LAST_FRAME as usize];

    let reference_mode = if compound_reference_allowed {
        let non_single_reference = r.flag();
        if !non_single_reference {
            SINGLE_REFERENCE
        } else {
            let reference_select = r.flag();
            if !reference_select {
                COMPOUND_REFERENCE
            } else {
                REFERENCE_MODE_SELECT
            }
        }
    } else {
        SINGLE_REFERENCE
    };

    let (comp_fixed_ref, comp_var_ref) = if reference_mode != SINGLE_REFERENCE {
        setup_compound_reference_mode(ref_frame_sign_bias)
    } else {
        (0, [0, 0])
    };

    (reference_mode, comp_fixed_ref, comp_var_ref)
}

/// `frame_reference_mode_probs()` (spec §6.3.13).
fn frame_reference_mode_probs(
    r: &mut BoolDecoder,
    reference_mode: u8,
    probs: &mut CompressedHeaderProbs,
) {
    if reference_mode == REFERENCE_MODE_SELECT {
        diff_update_prob_slice(r, &mut probs.comp_mode_prob);
    }
    if reference_mode != COMPOUND_REFERENCE {
        diff_update_prob_slice(r, probs.single_ref_prob.as_flattened_mut());
    }
    if reference_mode != SINGLE_REFERENCE {
        diff_update_prob_slice(r, &mut probs.comp_ref_prob);
    }
}

/// `update_mv_prob( prob )` (spec §6.3.17). Reads `B(252)` then `L(7)` directly.
fn update_mv_prob(r: &mut BoolDecoder, prob: u8) -> u8 {
    if r.read_bool(252) {
        ((r.read_literal(7) as u8) << 1) | 1
    } else {
        prob
    }
}

fn update_mv_prob_slice(r: &mut BoolDecoder, probs: &mut [u8]) {
    for prob in probs.iter_mut() {
        *prob = update_mv_prob(r, *prob);
    }
}

/// `mv_probs()` (spec §6.3.16).
fn mv_probs(r: &mut BoolDecoder, allow_high_precision_mv: bool, probs: &mut CompressedHeaderProbs) {
    update_mv_prob_slice(r, &mut probs.mv_joint_probs);
    for i in 0..2 {
        probs.mv_sign_prob[i] = update_mv_prob(r, probs.mv_sign_prob[i]);
        update_mv_prob_slice(r, &mut probs.mv_class_probs[i]);
        probs.mv_class0_bit_prob[i] = update_mv_prob(r, probs.mv_class0_bit_prob[i]);
        update_mv_prob_slice(r, &mut probs.mv_bits_prob[i]);
    }
    for i in 0..2 {
        update_mv_prob_slice(r, probs.mv_class0_fr_probs[i].as_flattened_mut());
        update_mv_prob_slice(r, &mut probs.mv_fr_probs[i]);
    }
    if allow_high_precision_mv {
        for i in 0..2 {
            probs.mv_class0_hp_prob[i] = update_mv_prob(r, probs.mv_class0_hp_prob[i]);
            probs.mv_hp_prob[i] = update_mv_prob(r, probs.mv_hp_prob[i]);
        }
    }
}

/// Parses `compressed_header()` (spec §6.3).
///
/// `data` is the `header_size_in_bytes`-byte slice. The five uncompressed-header-derived
/// values `read_tx_mode`/`frame_reference_mode`/etc. need (`Lossless`, `FrameIsIntra`,
/// `interpolation_filter`, `ref_frame_sign_bias`, `allow_high_precision_mv`) are all read
/// from `header` -- none of them are cross-frame state, unlike `starting_probs`:
/// - `header.quantization.lossless`: forces `tx_mode` to `ONLY_4X4` when set.
/// - `header.frame_is_intra`: when true, none of the inter-related syntax
///   (spec §6.3.9-6.3.18) is read at all.
/// - `header.interpolation_filter`: read_interp_filter_probs is skipped unless `SWITCHABLE`
///   (ignored entirely when `FrameIsIntra == 1`).
/// - `header.ref_frame_sign_bias`: indexed by `ref_frame` value.
/// - `header.allow_high_precision_mv`: gates whether `mv_class0_hp_prob`/`mv_hp_prob` are read.
/// - `starting_probs`: the starting probability table equivalent to
///   `load_probs( frame_context_idx )` (obtained via [`FrameContextStore::load`]).
pub fn parse_compressed_header(
    data: &[u8],
    header: &NewFrameHeader,
    starting_probs: FrameContext,
) -> Result<CompressedHeader, CompressedHeaderError> {
    let mut r = BoolDecoder::new(data)?;
    let mut probs = starting_probs;

    let tx_mode = read_tx_mode(&mut r, header.quantization.lossless);
    if tx_mode == TX_MODE_SELECT {
        read_tx_mode_probs(&mut r, &mut probs.tx_probs);
    }
    read_coef_probs(&mut r, tx_mode, &mut probs.coef_probs);
    diff_update_prob_slice(&mut r, &mut probs.skip_prob);

    let mut reference_mode = SINGLE_REFERENCE;
    let mut comp_fixed_ref = 0u8;
    let mut comp_var_ref = [0u8; 2];

    if !header.frame_is_intra {
        diff_update_prob_slice(&mut r, probs.inter_mode_probs.as_flattened_mut());
        if header.interpolation_filter == SWITCHABLE {
            diff_update_prob_slice(&mut r, probs.interp_filter_probs.as_flattened_mut());
        }
        diff_update_prob_slice(&mut r, &mut probs.is_inter_prob);
        (reference_mode, comp_fixed_ref, comp_var_ref) =
            frame_reference_mode(&mut r, &header.ref_frame_sign_bias);
        frame_reference_mode_probs(&mut r, reference_mode, &mut probs);
        diff_update_prob_slice(&mut r, probs.y_mode_probs.as_flattened_mut());
        diff_update_prob_slice(&mut r, probs.partition_probs.as_flattened_mut());
        mv_probs(&mut r, header.allow_high_precision_mv, &mut probs);
    }

    r.exit_bool();

    Ok(CompressedHeader {
        tx_mode,
        probs,
        reference_mode,
        comp_fixed_ref,
        comp_var_ref,
    })
}

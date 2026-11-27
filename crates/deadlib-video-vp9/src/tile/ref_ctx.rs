//! Pure neighbor-context derivation for reference-frame-related syntax elements (spec §9.3.2):
//! `comp_mode`/`comp_ref`/`single_ref_p1`/`single_ref_p2` contexts. Called from
//! `tile::mode_info::read_ref_frames`.

use super::mode_info::NeighborRefInfo;
use super::TileDecoder;
use crate::prob_tables::{ALTREF_FRAME, GOLDEN_FRAME, LAST_FRAME};

#[inline]
fn has_ref(rf: [u8; 2], target: u8) -> bool {
    rf[0] == target || rf[1] == target
}

#[inline]
fn var_ref(rf: [u8; 2], single: bool, var_ref_idx: usize) -> u8 {
    if single {
        rf[0]
    } else {
        rf[var_ref_idx]
    }
}

#[inline]
fn single_avail_comp_mode(rf: [u8; 2], single: bool, fixed: u8) -> usize {
    if single {
        (rf[0] == fixed) as usize
    } else {
        3
    }
}

#[inline]
fn single_avail_comp_ref(
    rf: [u8; 2],
    intra: bool,
    single: bool,
    var_ref_idx: usize,
    var1: u8,
) -> usize {
    if intra {
        2
    } else if single {
        3 * (rf[0] != var1) as usize
    } else {
        4 * (rf[var_ref_idx] != var1) as usize
    }
}

#[inline]
fn single_inter_p1(rf: [u8; 2], single: bool) -> usize {
    if single {
        4 * (rf[0] == LAST_FRAME) as usize
    } else {
        1 + has_ref(rf, LAST_FRAME) as usize
    }
}

#[inline]
fn single_avail_p1(rf: [u8; 2], intra: bool, single: bool) -> usize {
    if intra {
        2
    } else {
        single_inter_p1(rf, single)
    }
}

#[inline]
fn other_intra_p2(rf: [u8; 2], single: bool) -> usize {
    if single {
        if rf[0] == LAST_FRAME {
            3
        } else {
            4 * (rf[0] == GOLDEN_FRAME) as usize
        }
    } else {
        1 + 2 * has_ref(rf, GOLDEN_FRAME) as usize
    }
}

#[inline]
fn single_avail_p2(rf: [u8; 2], intra: bool, single: bool) -> usize {
    if intra || (rf[0] == LAST_FRAME && single) {
        2
    } else if single {
        4 * (rf[0] == GOLDEN_FRAME) as usize
    } else {
        3 * has_ref(rf, GOLDEN_FRAME) as usize
    }
}

impl TileDecoder {
    /// Context derivation for `comp_mode` (spec §9.3.2).
    pub(super) fn comp_mode_ctx(&self, n: &NeighborRefInfo) -> usize {
        let fixed = self.comp_fixed_ref;
        match (n.avail_u, n.avail_l) {
            (true, true) => match (n.above_single, n.left_single) {
                (true, true) => {
                    ((n.above_ref_frame[0] == fixed) ^ (n.left_ref_frame[0] == fixed)) as usize
                }
                (true, false) => 2 + (n.above_ref_frame[0] == fixed || n.above_intra) as usize,
                (false, true) => 2 + (n.left_ref_frame[0] == fixed || n.left_intra) as usize,
                (false, false) => 4,
            },
            (true, false) => single_avail_comp_mode(n.above_ref_frame, n.above_single, fixed),
            (false, true) => single_avail_comp_mode(n.left_ref_frame, n.left_single, fixed),
            (false, false) => 1,
        }
    }

    /// Context derivation for `comp_ref` (spec §9.3.2).
    pub(super) fn comp_ref_ctx(&self, n: &NeighborRefInfo) -> usize {
        let fix_ref_idx = self.ref_frame_sign_bias[self.comp_fixed_ref as usize] as usize;
        let var_ref_idx = 1 - fix_ref_idx;
        let var0 = self.comp_var_ref[0];
        let var1 = self.comp_var_ref[1];

        match (n.avail_u, n.avail_l) {
            (true, true) => match (n.above_intra, n.left_intra) {
                (true, true) => 2,
                (false, true) => {
                    1 + 2
                        * (var_ref(n.above_ref_frame, n.above_single, var_ref_idx) != var1) as usize
                }
                (true, false) => {
                    1 + 2 * (var_ref(n.left_ref_frame, n.left_single, var_ref_idx) != var1) as usize
                }
                (false, false) => {
                    let vrfa = var_ref(n.above_ref_frame, n.above_single, var_ref_idx);
                    let vrfl = var_ref(n.left_ref_frame, n.left_single, var_ref_idx);
                    if vrfa == vrfl && var1 == vrfa {
                        0
                    } else if n.left_single && n.above_single {
                        if (vrfa == self.comp_fixed_ref && vrfl == var0)
                            || (vrfl == self.comp_fixed_ref && vrfa == var0)
                        {
                            4
                        } else if vrfa == vrfl {
                            3
                        } else {
                            1
                        }
                    } else if n.left_single || n.above_single {
                        let (vrfc, rfs) = if n.left_single {
                            (vrfa, vrfl)
                        } else {
                            (vrfl, vrfa)
                        };
                        if vrfc == var1 && rfs != var1 {
                            1
                        } else if rfs == var1 && vrfc != var1 {
                            2
                        } else {
                            4
                        }
                    } else if vrfa == vrfl {
                        4
                    } else {
                        2
                    }
                }
            },
            (true, false) => single_avail_comp_ref(
                n.above_ref_frame,
                n.above_intra,
                n.above_single,
                var_ref_idx,
                var1,
            ),
            (false, true) => single_avail_comp_ref(
                n.left_ref_frame,
                n.left_intra,
                n.left_single,
                var_ref_idx,
                var1,
            ),
            (false, false) => 2,
        }
    }

    /// Context derivation for `single_ref_p1` (spec §9.3.2).
    pub(super) fn single_ref_p1_ctx(&self, n: &NeighborRefInfo) -> usize {
        match (n.avail_u, n.avail_l) {
            (true, true) => match (n.above_intra, n.left_intra) {
                (true, true) => 2,
                (false, true) => single_inter_p1(n.above_ref_frame, n.above_single),
                (true, false) => single_inter_p1(n.left_ref_frame, n.left_single),
                (false, false) => match (n.above_single, n.left_single) {
                    (true, true) => {
                        2 * (n.above_ref_frame[0] == LAST_FRAME) as usize
                            + 2 * (n.left_ref_frame[0] == LAST_FRAME) as usize
                    }
                    (false, false) => {
                        1 + (has_ref(n.above_ref_frame, LAST_FRAME)
                            || has_ref(n.left_ref_frame, LAST_FRAME))
                            as usize
                    }
                    (above_single, _) => {
                        let (rfs, crf) = if above_single {
                            (n.above_ref_frame[0], n.left_ref_frame)
                        } else {
                            (n.left_ref_frame[0], n.above_ref_frame)
                        };
                        let crf_last = has_ref(crf, LAST_FRAME) as usize;
                        if rfs == LAST_FRAME {
                            3 + crf_last
                        } else {
                            crf_last
                        }
                    }
                },
            },
            (true, false) => single_avail_p1(n.above_ref_frame, n.above_intra, n.above_single),
            (false, true) => single_avail_p1(n.left_ref_frame, n.left_intra, n.left_single),
            (false, false) => 2,
        }
    }

    /// Context derivation for `single_ref_p2` (spec §9.3.2).
    pub(super) fn single_ref_p2_ctx(&self, n: &NeighborRefInfo) -> usize {
        match (n.avail_u, n.avail_l) {
            (true, true) => match (n.above_intra, n.left_intra) {
                (true, true) => 2,
                (false, true) => other_intra_p2(n.above_ref_frame, n.above_single),
                (true, false) => other_intra_p2(n.left_ref_frame, n.left_single),
                (false, false) => match (n.above_single, n.left_single) {
                    (true, true) => {
                        match (
                            n.above_ref_frame[0] == LAST_FRAME,
                            n.left_ref_frame[0] == LAST_FRAME,
                        ) {
                            (true, true) => 3,
                            (true, false) => 4 * (n.left_ref_frame[0] == GOLDEN_FRAME) as usize,
                            (false, true) => 4 * (n.above_ref_frame[0] == GOLDEN_FRAME) as usize,
                            (false, false) => {
                                2 * (n.above_ref_frame[0] == GOLDEN_FRAME) as usize
                                    + 2 * (n.left_ref_frame[0] == GOLDEN_FRAME) as usize
                            }
                        }
                    }
                    (false, false) => {
                        if n.above_ref_frame == n.left_ref_frame {
                            3 * has_ref(n.above_ref_frame, GOLDEN_FRAME) as usize
                        } else {
                            2
                        }
                    }
                    (above_single, _) => {
                        let (rfs, crf) = if above_single {
                            (n.above_ref_frame[0], n.left_ref_frame)
                        } else {
                            (n.left_ref_frame[0], n.above_ref_frame)
                        };
                        let crf_gold = has_ref(crf, GOLDEN_FRAME) as usize;
                        if rfs == GOLDEN_FRAME {
                            3 + crf_gold
                        } else if rfs == ALTREF_FRAME {
                            crf_gold
                        } else {
                            1 + 2 * crf_gold
                        }
                    }
                },
            },
            (true, false) => single_avail_p2(n.above_ref_frame, n.above_intra, n.above_single),
            (false, true) => single_avail_p2(n.left_ref_frame, n.left_intra, n.left_single),
            (false, false) => 2,
        }
    }
}

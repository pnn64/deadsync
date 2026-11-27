//! Prediction processes: intra prediction (spec §8.5.1, `intra`) and inter prediction
//! (spec §8.5.2, `inter`). Both are called per plane by [`crate::tile::TileDecoder`]'s
//! residual path (`src/tile/residual.rs`).

mod inter;
mod intra;

#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
pub(crate) use inter::RefSample;
pub use inter::{predict_inter, InterPredictParams, RefPlaneView};
pub use intra::predict_intra;

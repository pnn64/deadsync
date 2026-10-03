// Frozen from be7c69c5d (0.5.1702).
use super::*;

pub(super) fn encode_local_score_index(index: &LocalScoreIndex) -> Option<Vec<u8>> {
    bincode::encode_to_vec(
        LocalScoreIndexFile {
            version: LOCAL_SCORE_INDEX_VERSION,
            index: index.clone(),
        },
        bincode::config::standard(),
    )
    .ok()
}

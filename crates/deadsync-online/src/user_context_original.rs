// Frozen from main 6b440b74a for behavior and paired performance comparisons.
use super::*;

pub fn user_context_from_api(user: ArrowCloudUserApiUser) -> ArrowCloudUserContext {
    let self_user_id = arrowcloud_user_id(user.id.as_str()).map(str::to_string);
    let rival_user_ids = user
        .rival_user_ids
        .into_iter()
        .map(|user_id| user_id.trim().to_string())
        .filter(|user_id| !user_id.is_empty())
        .collect();
    ArrowCloudUserContext {
        self_user_id,
        rival_user_ids,
    }
}

pub fn update_remaining_targets(
    scores: &[ArrowCloudLeaderboardEntry],
    context: Option<&ArrowCloudUserContext>,
    remaining: &mut HashSet<String>,
) {
    if remaining.is_empty() {
        return;
    }
    for entry in scores {
        let Some(user_id) = arrowcloud_user_id(entry.user_id.as_str()) else {
            continue;
        };
        let (is_self, is_rival) =
            arrowcloud_entry_flags(Some(user_id), entry.is_self, entry.is_rival, context);
        if is_self || is_rival {
            remaining.remove(user_id);
            if remaining.is_empty() {
                break;
            }
        }
    }
}

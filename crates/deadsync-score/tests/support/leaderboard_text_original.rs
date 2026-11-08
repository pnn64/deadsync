// Frozen function bodies from 2d1ef2b30.
use deadsync_score::leaderboard::{ArrowCloudPaneKind, PlayerLeaderboardCacheKey};
use std::collections::HashMap;

pub fn scorebox_machine_tag(machine_tag: Option<&str>, name: &str) -> String {
    let src = machine_tag.unwrap_or(name).trim();
    if src.is_empty() {
        return "----".to_string();
    }
    let mut out = String::with_capacity(4);
    for ch in src.chars().take(4) {
        out.push(ch.to_ascii_uppercase());
    }
    out
}

pub fn arrowcloud_pane_kind_from_type(lb_type: &str) -> Option<ArrowCloudPaneKind> {
    if lb_type.is_empty() {
        return None;
    }
    let mut compact = String::with_capacity(lb_type.len());
    for ch in lb_type.chars() {
        if ch.is_ascii_alphanumeric() {
            compact.push(ch.to_ascii_lowercase());
        }
    }
    match compact.as_str() {
        "itg" => Some(ArrowCloudPaneKind::Itg),
        "ex" => Some(ArrowCloudPaneKind::Ex),
        "hardex" | "hex" => Some(ArrowCloudPaneKind::HardEx),
        _ => None,
    }
}

pub fn queue_player_leaderboard_refresh(
    pending_refresh: &mut HashMap<PlayerLeaderboardCacheKey, usize>,
    key: &PlayerLeaderboardCacheKey,
    requested_max_entries: usize,
) {
    pending_refresh
        .entry(key.clone())
        .and_modify(|max_entries| *max_entries = (*max_entries).max(requested_max_entries))
        .or_insert(requested_max_entries);
}

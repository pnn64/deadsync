// Frozen function bodies from 48a969486.
use deadsync_score::leaderboard::scorebox_score_percent;

pub fn format_scorebox_score_percent(score_10000: f64) -> String {
    format!("{:.2}%", scorebox_score_percent(score_10000))
}

pub fn format_scorebox_score_value(score_10000: f64) -> String {
    format!("{:.2}", scorebox_score_percent(score_10000))
}

pub fn format_scorebox_rank(rank: u32) -> String {
    format!("{rank}.")
}

pub fn leaderboard_username_matches(entry_name: &str, username: &str) -> bool {
    !username.trim().is_empty() && entry_name.eq_ignore_ascii_case(username)
}

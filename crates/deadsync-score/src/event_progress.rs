use crate::LeaderboardEntry;
use smallvec::SmallVec;
use std::fmt::Write as _;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventProgressKind {
    #[default]
    Itl,
    Srpg,
}

#[derive(Clone, Debug)]
pub struct EventStatImprovement {
    pub name: String,
    pub gained: u32,
    pub current: i32,
}

#[derive(Clone, Debug)]
pub enum EventOverlayPage {
    Text(String),
    Leaderboard(Vec<LeaderboardEntry>),
}

#[derive(Clone, Debug, Default)]
pub struct EventProgress {
    pub kind: EventProgressKind,
    pub name: String,
    pub is_doubles: bool,
    pub score_hundredths: u32,
    pub score_delta_hundredths: i32,
    pub rate_hundredths: Option<u32>,
    pub rate_delta_hundredths: Option<i32>,
    pub current_points: u32,
    pub point_delta: i32,
    pub current_ranking_points: u32,
    pub ranking_delta: i32,
    pub current_song_points: u32,
    pub song_delta: i32,
    pub current_ex_points: u32,
    pub ex_delta: i32,
    pub current_total_points: u32,
    pub total_delta: i32,
    pub total_passes: u32,
    pub clear_type_before: Option<u8>,
    pub clear_type_after: Option<u8>,
    pub stat_improvements: Vec<EventStatImprovement>,
    pub skill_improvements: Vec<String>,
    pub overlay_pages: Vec<EventOverlayPage>,
}

pub type ItlEventProgress = EventProgress;
pub type ItlOverlayPage = EventOverlayPage;

type RewardDescriptions<'a> = SmallVec<[&'a str; 4]>;
type RewardGroups<'a> = SmallVec<[(&'a str, RewardDescriptions<'a>); 4]>;

#[derive(Clone, Debug, Default)]
pub struct SubmitStatImprovement {
    pub name: String,
    pub gained: u32,
    pub current: i32,
}

#[derive(Clone, Debug, Default)]
pub struct SubmitQuestReward {
    pub reward_type: String,
    pub description: String,
}

#[derive(Clone, Debug, Default)]
pub struct SubmitQuest {
    pub title: String,
    pub rewards: Vec<SubmitQuestReward>,
}

#[derive(Clone, Debug, Default)]
pub struct SubmitAchievementReward {
    pub tier: String,
    pub requirements: Vec<String>,
    pub title_unlocked: String,
}

#[derive(Clone, Debug, Default)]
pub struct SubmitAchievement {
    pub title: String,
    pub rewards: Vec<SubmitAchievementReward>,
}

#[derive(Clone, Debug, Default)]
pub struct SubmitProgress {
    pub stat_improvements: Vec<SubmitStatImprovement>,
    pub skill_improvements: Vec<String>,
    pub quests_completed: Vec<SubmitQuest>,
    pub achievements_completed: Vec<SubmitAchievement>,
}

#[derive(Clone, Debug, Default)]
pub struct SubmitEventProgressData {
    pub name: String,
    pub is_doubles: bool,
    pub score_delta: i32,
    pub rate_delta: i32,
    pub top_score_points: u32,
    pub prev_top_score_points: u32,
    pub total_passes: u32,
    pub current_ranking_point_total: u32,
    pub previous_ranking_point_total: u32,
    pub current_song_point_total: u32,
    pub previous_song_point_total: u32,
    pub current_ex_point_total: u32,
    pub previous_ex_point_total: u32,
    pub current_point_total: u32,
    pub previous_point_total: u32,
    pub leaderboard: Vec<LeaderboardEntry>,
    pub progress: Option<SubmitProgress>,
}

#[derive(Clone, Debug, Default)]
pub struct SubmitEventProgressInput {
    pub result: String,
    pub score_10000: u32,
    pub rate_hundredths: u32,
    pub itl_score_hundredths: Option<u32>,
    pub itl: Option<SubmitEventProgressData>,
    pub srpg: Option<SubmitEventProgressData>,
}

#[must_use]
pub fn event_name_or_unknown(name: &str) -> &str {
    if name.trim().is_empty() {
        "Unknown Event"
    } else {
        name.trim()
    }
}

#[inline(always)]
#[must_use]
pub const fn clear_type_name(clear_type: u8) -> &'static str {
    match clear_type {
        0 => "No Play",
        1 => "Clear",
        2 => "FC",
        3 => "FEC",
        4 => "FFC",
        5 => "FBFC",
        _ => "Clear",
    }
}

#[inline(always)]
#[must_use]
pub fn delta_i32(current: u32, previous: u32) -> i32 {
    (i64::from(current) - i64::from(previous)).clamp(i64::from(i32::MIN), i64::from(i32::MAX))
        as i32
}

fn trim_blank_lines(mut text: String) -> String {
    text.truncate(text.trim_end_matches(['\n', '\r']).len());
    text
}

fn push_capitalized_first(out: &mut String, text: &str) {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return;
    };
    out.extend(first.to_uppercase());
    out.extend(chars);
}

fn push_stat_improvement_lines(out: &mut String, progress: Option<&SubmitProgress>) -> bool {
    let Some(progress) = progress else {
        return false;
    };
    let mut wrote_line = false;
    for improvement in &progress.stat_improvements {
        if improvement.gained == 0 {
            continue;
        }
        if improvement.name.eq_ignore_ascii_case("clearType") {
            let after = improvement.current.clamp(0, i32::from(u8::MAX)) as u8;
            let before = after.saturating_sub(improvement.gained.min(u32::from(u8::MAX)) as u8);
            if wrote_line {
                out.push('\n');
            }
            let _ = write!(
                out,
                "Clear Type: {} >>> {}",
                clear_type_name(before),
                clear_type_name(after)
            );
            wrote_line = true;
            continue;
        }
        if improvement.name.eq_ignore_ascii_case("grade") {
            let curr = improvement.current;
            let prev = curr - improvement.gained as i32;
            if curr != 0 && prev != curr {
                let grade = match curr {
                    1 => Some("Quad"),
                    2 => Some("Quint"),
                    _ => None,
                };
                if let Some(grade) = grade {
                    if wrote_line {
                        out.push('\n');
                    }
                    let _ = write!(out, "New {grade}!");
                    wrote_line = true;
                }
            }
            continue;
        }
        if wrote_line {
            out.push('\n');
        }
        push_capitalized_first(out, improvement.name.trim_end_matches("Level"));
        let _ = write!(
            out,
            " Lvl: {} (+{})",
            improvement.current, improvement.gained
        );
        wrote_line = true;
    }
    wrote_line
}

fn push_uppercase(out: &mut String, text: &str) {
    for ch in text.chars() {
        out.extend(ch.to_uppercase());
    }
}

fn push_srpg_stat_improvement_lines(out: &mut String, progress: &ItlEventProgress) -> bool {
    let mut wrote_line = false;
    for improvement in &progress.stat_improvements {
        if improvement.gained == 0 {
            continue;
        }
        if wrote_line {
            out.push('\n');
        }
        let _ = write!(out, "+{} ", improvement.gained);
        push_uppercase(out, improvement.name.as_str());
        wrote_line = true;
    }
    wrote_line
}

fn push_joined_lines(out: &mut String, lines: &[String]) {
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(line);
    }
}

fn srpg_summary_page_text(progress: &ItlEventProgress) -> String {
    let rate = progress.rate_hundredths.unwrap_or(100);
    let rate_delta = progress.rate_delta_hundredths.unwrap_or(0);
    let mut text = format!(
        "Skill Improvements\n\n\
         {:.2}% ({:+.2}%) at\n\
         {:.2}x ({:+.2}x) rate",
        f64::from(progress.score_hundredths) / 100.0,
        f64::from(progress.score_delta_hundredths) / 100.0,
        f64::from(rate) / 100.0,
        f64::from(rate_delta) / 100.0,
    );
    let stat_start = text.len();
    text.push_str("\n\n");
    if !push_srpg_stat_improvement_lines(&mut text, progress) {
        text.truncate(stat_start);
    }
    if !progress.skill_improvements.is_empty() {
        text.push_str("\n\n");
        push_joined_lines(&mut text, progress.skill_improvements.as_slice());
    }
    trim_blank_lines(text)
}

fn itl_summary_page_text(
    progress: &ItlEventProgress,
    submit_progress: Option<&SubmitProgress>,
) -> String {
    let mut text = format!(
        "EX Score: {:.2}% ({:+.2}%)\n\
         Points: {} ({:+})\n\n\
         Ranking Points: {} ({:+})\n\
         Song Points: {} ({:+})\n\
         EX Points: {} ({:+})\n\
         Total Points: {} ({:+})\n\n\
         You've passed the chart {} times",
        f64::from(progress.score_hundredths) / 100.0,
        f64::from(progress.score_delta_hundredths) / 100.0,
        progress.current_points,
        progress.point_delta,
        progress.current_ranking_points,
        progress.ranking_delta,
        progress.current_song_points,
        progress.song_delta,
        progress.current_ex_points,
        progress.ex_delta,
        progress.current_total_points,
        progress.total_delta,
        progress.total_passes,
    );
    let stat_start = text.len();
    text.push_str("\n\n");
    if !push_stat_improvement_lines(&mut text, submit_progress) {
        text.truncate(stat_start);
    }
    trim_blank_lines(text)
}

fn summary_page_text(
    progress: &ItlEventProgress,
    submit_progress: Option<&SubmitProgress>,
) -> String {
    match progress.kind {
        EventProgressKind::Itl => itl_summary_page_text(progress, submit_progress),
        EventProgressKind::Srpg => srpg_summary_page_text(progress),
    }
}

fn append_grouped_reward_text(out: &mut String, reward_type: &str, descriptions: &[&str]) {
    if descriptions.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    if !reward_type.eq_ignore_ascii_case("ad-hoc") {
        push_uppercase(out, reward_type.trim());
        out.push_str(":\n");
    }
    for (index, description) in descriptions.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(description);
    }
}

fn quest_page_text(quest: &SubmitQuest) -> String {
    let mut body = format!("Completed \"{}\"!", quest.title.trim());
    let mut grouped = RewardGroups::new();
    for reward in &quest.rewards {
        let reward_type = reward.reward_type.trim();
        let description = reward.description.trim();
        if description.is_empty() {
            continue;
        }
        if let Some((_, descriptions)) = grouped
            .iter_mut()
            .find(|(kind, _)| kind.eq_ignore_ascii_case(reward_type))
        {
            descriptions.push(description);
        } else {
            grouped.push((reward_type, SmallVec::from_slice(&[description])));
        }
    }
    for (reward_type, descriptions) in &grouped {
        append_grouped_reward_text(&mut body, reward_type, descriptions.as_slice());
    }
    trim_blank_lines(body)
}

fn achievement_page_text(achievement: &SubmitAchievement) -> String {
    let mut text = format!(
        "Completed the \"{}\" Achievement!",
        achievement.title.trim()
    );
    for reward in &achievement.rewards {
        let tier = reward.tier.trim();
        if !tier.is_empty() && tier != "0" {
            let _ = write!(text, "\nTier {tier}");
        }
        for requirement in &reward.requirements {
            let requirement = requirement.trim();
            if !requirement.is_empty() {
                text.push('\n');
                text.push_str(requirement);
            }
        }
        let title = reward.title_unlocked.trim();
        if !title.is_empty() {
            let _ = write!(text, "\nUnlocked the \"{title}\" Title!");
        }
        text.push('\n');
    }
    trim_blank_lines(text)
}

#[must_use]
pub fn event_progress_overlay_pages(
    progress: &ItlEventProgress,
    submit_progress: Option<&SubmitProgress>,
    leaderboard: &[LeaderboardEntry],
) -> Vec<ItlOverlayPage> {
    let mut pages = vec![ItlOverlayPage::Text(summary_page_text(
        progress,
        submit_progress,
    ))];
    let Some(submit_progress) = submit_progress else {
        pages.push(ItlOverlayPage::Leaderboard(leaderboard.to_vec()));
        return pages;
    };
    for quest in &submit_progress.quests_completed {
        pages.push(ItlOverlayPage::Text(quest_page_text(quest)));
    }
    for achievement in &submit_progress.achievements_completed {
        pages.push(ItlOverlayPage::Text(achievement_page_text(achievement)));
    }
    pages.push(ItlOverlayPage::Leaderboard(leaderboard.to_vec()));
    pages
}

#[must_use]
pub fn event_clear_type_change(progress: Option<&SubmitProgress>) -> (Option<u8>, Option<u8>) {
    let Some(progress) = progress else {
        return (None, None);
    };
    for improvement in &progress.stat_improvements {
        if improvement.gained == 0 || !improvement.name.eq_ignore_ascii_case("clearType") {
            continue;
        }
        let after = improvement.current.clamp(0, i32::from(u8::MAX)) as u8;
        let before = after.saturating_sub(improvement.gained.min(u32::from(u8::MAX)) as u8);
        return (Some(before), Some(after));
    }
    (None, None)
}

#[must_use]
pub fn event_stat_improvements(progress: Option<&SubmitProgress>) -> Vec<EventStatImprovement> {
    let Some(progress) = progress else {
        return Vec::new();
    };
    progress
        .stat_improvements
        .iter()
        .filter(|improvement| improvement.gained > 0)
        .map(|improvement| EventStatImprovement {
            name: improvement.name.clone(),
            gained: improvement.gained,
            current: improvement.current,
        })
        .collect()
}

fn itl_progress_from_submit(input: &SubmitEventProgressInput) -> Option<ItlEventProgress> {
    let itl = input.itl.as_ref()?;
    let score_hundredths = input.itl_score_hundredths?;
    let (clear_type_before, clear_type_after) = event_clear_type_change(itl.progress.as_ref());
    let mut progress = ItlEventProgress {
        kind: EventProgressKind::Itl,
        name: event_name_or_unknown(itl.name.as_str()).to_string(),
        is_doubles: itl.is_doubles,
        score_hundredths,
        score_delta_hundredths: itl.score_delta,
        rate_hundredths: None,
        rate_delta_hundredths: None,
        current_points: itl.top_score_points,
        point_delta: delta_i32(itl.top_score_points, itl.prev_top_score_points),
        current_ranking_points: itl.current_ranking_point_total,
        ranking_delta: delta_i32(
            itl.current_ranking_point_total,
            itl.previous_ranking_point_total,
        ),
        current_song_points: itl.current_song_point_total,
        song_delta: delta_i32(itl.current_song_point_total, itl.previous_song_point_total),
        current_ex_points: itl.current_ex_point_total,
        ex_delta: delta_i32(itl.current_ex_point_total, itl.previous_ex_point_total),
        current_total_points: itl.current_point_total,
        total_delta: delta_i32(itl.current_point_total, itl.previous_point_total),
        total_passes: itl.total_passes,
        clear_type_before,
        clear_type_after,
        stat_improvements: event_stat_improvements(itl.progress.as_ref()),
        skill_improvements: Vec::new(),
        overlay_pages: Vec::new(),
    };
    progress.overlay_pages =
        event_progress_overlay_pages(&progress, itl.progress.as_ref(), itl.leaderboard.as_slice());
    Some(progress)
}

fn srpg_progress_from_submit(input: &SubmitEventProgressInput) -> Option<ItlEventProgress> {
    let srpg = input.srpg.as_ref()?;
    let score_delta = if input.result.eq_ignore_ascii_case("score-added") {
        input.score_10000 as i32
    } else {
        srpg.score_delta
    };
    let rate_delta = if input.result.eq_ignore_ascii_case("score-added") {
        input.rate_hundredths as i32
    } else {
        srpg.rate_delta
    };
    let mut progress = ItlEventProgress {
        kind: EventProgressKind::Srpg,
        name: event_name_or_unknown(srpg.name.as_str()).to_string(),
        is_doubles: srpg.is_doubles,
        score_hundredths: input.score_10000,
        score_delta_hundredths: score_delta,
        rate_hundredths: Some(input.rate_hundredths),
        rate_delta_hundredths: Some(rate_delta),
        current_points: srpg.top_score_points,
        point_delta: delta_i32(srpg.top_score_points, srpg.prev_top_score_points),
        current_ranking_points: srpg.current_ranking_point_total,
        ranking_delta: delta_i32(
            srpg.current_ranking_point_total,
            srpg.previous_ranking_point_total,
        ),
        current_song_points: srpg.current_song_point_total,
        song_delta: delta_i32(
            srpg.current_song_point_total,
            srpg.previous_song_point_total,
        ),
        current_ex_points: srpg.current_ex_point_total,
        ex_delta: delta_i32(srpg.current_ex_point_total, srpg.previous_ex_point_total),
        current_total_points: srpg.current_point_total,
        total_delta: delta_i32(srpg.current_point_total, srpg.previous_point_total),
        total_passes: srpg.total_passes,
        clear_type_before: None,
        clear_type_after: None,
        stat_improvements: event_stat_improvements(srpg.progress.as_ref()),
        skill_improvements: srpg
            .progress
            .as_ref()
            .map(|progress| progress.skill_improvements.clone())
            .unwrap_or_default(),
        overlay_pages: Vec::new(),
    };
    progress.overlay_pages = event_progress_overlay_pages(
        &progress,
        srpg.progress.as_ref(),
        srpg.leaderboard.as_slice(),
    );
    Some(progress)
}

#[must_use]
pub fn event_progress_from_submit(input: &SubmitEventProgressInput) -> Vec<ItlEventProgress> {
    let mut progress = Vec::with_capacity(2);
    if let Some(srpg) = srpg_progress_from_submit(input) {
        progress.push(srpg);
    }
    if let Some(itl) = itl_progress_from_submit(input) {
        progress.push(itl);
    }
    progress
}

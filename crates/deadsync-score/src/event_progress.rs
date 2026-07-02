use crate::LeaderboardEntry;

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

pub fn event_name_or_unknown(name: &str) -> &str {
    if name.trim().is_empty() {
        "Unknown Event"
    } else {
        name.trim()
    }
}

#[inline(always)]
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
pub fn delta_i32(current: u32, previous: u32) -> i32 {
    (i64::from(current) - i64::from(previous)).clamp(i64::from(i32::MIN), i64::from(i32::MAX))
        as i32
}

fn trim_blank_lines(text: String) -> String {
    text.trim_end_matches(['\n', '\r']).to_string()
}

fn capitalize_ascii_first(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::new();
    out.extend(first.to_uppercase());
    out.extend(chars);
    out
}

fn stat_improvement_lines(progress: Option<&SubmitProgress>) -> Vec<String> {
    let Some(progress) = progress else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for improvement in &progress.stat_improvements {
        if improvement.gained == 0 {
            continue;
        }
        if improvement.name.eq_ignore_ascii_case("clearType") {
            let after = improvement.current.clamp(0, i32::from(u8::MAX)) as u8;
            let before = after.saturating_sub(improvement.gained.min(u32::from(u8::MAX)) as u8);
            lines.push(format!(
                "Clear Type: {} >>> {}",
                clear_type_name(before),
                clear_type_name(after)
            ));
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
                    lines.push(format!("New {grade}!"));
                }
            }
            continue;
        }
        let stat_name = capitalize_ascii_first(improvement.name.trim_end_matches("Level"));
        lines.push(format!(
            "{stat_name} Lvl: {} (+{})",
            improvement.current, improvement.gained
        ));
    }
    lines
}

fn srpg_stat_improvement_lines(progress: &ItlEventProgress) -> Vec<String> {
    progress
        .stat_improvements
        .iter()
        .filter(|improvement| improvement.gained > 0)
        .map(|improvement| {
            format!(
                "+{} {}",
                improvement.gained,
                improvement.name.to_uppercase()
            )
        })
        .collect()
}

fn srpg_summary_page_text(progress: &ItlEventProgress) -> String {
    let rate = progress.rate_hundredths.unwrap_or(100);
    let rate_delta = progress.rate_delta_hundredths.unwrap_or(0);
    let mut text = format!(
        "Skill Improvements\n\n\
         {:.2}% ({:+.2}%) at\n\
         {:.2}x ({:+.2}x) rate",
        progress.score_hundredths as f64 / 100.0,
        progress.score_delta_hundredths as f64 / 100.0,
        rate as f64 / 100.0,
        rate_delta as f64 / 100.0,
    );
    let lines = srpg_stat_improvement_lines(progress);
    if !lines.is_empty() {
        text.push_str("\n\n");
        text.push_str(lines.join("\n").as_str());
    }
    if !progress.skill_improvements.is_empty() {
        text.push_str("\n\n");
        text.push_str(progress.skill_improvements.join("\n").as_str());
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
        progress.score_hundredths as f64 / 100.0,
        progress.score_delta_hundredths as f64 / 100.0,
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
    let lines = stat_improvement_lines(submit_progress);
    if !lines.is_empty() {
        text.push_str("\n\n");
        text.push_str(lines.join("\n").as_str());
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

fn append_grouped_reward_text(out: &mut String, reward_type: &str, descriptions: &[String]) {
    if descriptions.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    if !reward_type.eq_ignore_ascii_case("ad-hoc") {
        out.push_str(reward_type.trim().to_ascii_uppercase().as_str());
        out.push_str(":\n");
    }
    out.push_str(descriptions.join("\n").as_str());
}

fn quest_page_text(quest: &SubmitQuest) -> String {
    let mut body = format!("Completed \"{}\"!", quest.title.trim());
    let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
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
            descriptions.push(description.to_string());
        } else {
            grouped.push((reward_type.to_string(), vec![description.to_string()]));
        }
    }
    for (reward_type, descriptions) in &grouped {
        append_grouped_reward_text(&mut body, reward_type.as_str(), descriptions.as_slice());
    }
    trim_blank_lines(body)
}

fn achievement_page_text(achievement: &SubmitAchievement) -> String {
    let mut lines = vec![format!(
        "Completed the \"{}\" Achievement!",
        achievement.title.trim()
    )];
    for reward in &achievement.rewards {
        let tier = reward.tier.trim();
        if !tier.is_empty() && tier != "0" {
            lines.push(format!("Tier {tier}"));
        }
        for requirement in &reward.requirements {
            let requirement = requirement.trim();
            if !requirement.is_empty() {
                lines.push(requirement.to_string());
            }
        }
        let title = reward.title_unlocked.trim();
        if !title.is_empty() {
            lines.push(format!("Unlocked the \"{}\" Title!", title));
        }
        lines.push(String::new());
    }
    trim_blank_lines(lines.join("\n"))
}

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

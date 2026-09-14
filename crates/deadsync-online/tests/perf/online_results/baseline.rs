// Frozen from 231caabac / 0.5.1223; unchanged algorithms.
use super::*;
use smallvec::SmallVec;
type RewardDescriptions<'a> = SmallVec<[&'a str; 4]>;
type RewardGroups<'a> = SmallVec<[(&'a str, RewardDescriptions<'a>); 4]>;

pub fn hard_ex_pane_from_pages(
    first_page: ArrowCloudLeaderboardPane,
    extra_pages: Vec<ArrowCloudLeaderboardPane>,
    context: Option<&ArrowCloudUserContext>,
) -> LeaderboardPane {
    let mut entries = Vec::with_capacity(first_page.scores.len());
    let mut appended_user_ids = HashSet::new();

    for entry in first_page.scores {
        let user_id = arrowcloud_user_id(entry.user_id.as_str()).map(str::to_owned);
        let (is_self, is_rival) =
            arrowcloud_entry_flags(user_id.as_deref(), entry.is_self, entry.is_rival, context);
        if (is_self || is_rival)
            && let Some(user_id) = user_id
        {
            appended_user_ids.insert(user_id);
        }
        entries.push(leaderboard_entry_from_api(entry, is_self, is_rival));
    }

    for page in extra_pages {
        for entry in page.scores {
            let user_id = arrowcloud_user_id(entry.user_id.as_str()).map(str::to_owned);
            let (is_self, is_rival) =
                arrowcloud_entry_flags(user_id.as_deref(), entry.is_self, entry.is_rival, context);
            if !(is_self || is_rival) {
                continue;
            }
            if let Some(user_id) = user_id
                && !appended_user_ids.insert(user_id)
            {
                continue;
            }
            entries.push(leaderboard_entry_from_api(entry, is_self, is_rival));
        }
    }
    let personalized = entries.iter().any(|entry| entry.is_self || entry.is_rival);

    arrowcloud_hard_ex_leaderboard_pane(entries, personalized)
}

pub fn score_from_retrieve_entry(entry: &ArrowCloudRetrieveScoreEntry) -> Option<ArrowCloudScore> {
    arrowcloud_score_from_retrieve_fields(
        entry.score,
        entry.grade.as_deref(),
        entry.date.as_deref(),
        entry.play_id,
        entry.is_fail,
    )
}

pub fn scores_from_retrieve_entry_map(
    leaderboards: &HashMap<String, ArrowCloudRetrieveScoreEntry>,
) -> ArrowCloudScores {
    let mut out = ArrowCloudScores::default();
    for (leaderboard_id, entry) in leaderboards {
        let Ok(leaderboard_id) = leaderboard_id.parse::<u32>() else {
            continue;
        };
        let Some(score) = score_from_retrieve_entry(entry) else {
            continue;
        };
        set_arrowcloud_score_for_leaderboard(&mut out, leaderboard_id, score);
    }
    out
}

pub fn event_progress_from_submit_response(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
) -> Vec<ItlEventProgress> {
    let input = SubmitEventProgressInput {
        result: response.result.clone(),
        score_10000: player.score_10000,
        rate_hundredths: player.rate_hundredths,
        itl_score_hundredths: player.itl_score_hundredths,
        itl: response
            .itl
            .as_ref()
            .map(|event| submit_event_progress_from_api(event, event.itl_leaderboard.clone())),
        srpg: response
            .srpg
            .as_ref()
            .map(|event| submit_event_progress_from_api(event, event.srpg_leaderboard.clone())),
    };
    event_progress_from_submit(&input)
}

pub fn submit_event_progress_from_api(
    event: &GrooveStatsSubmitApiEvent,
    leaderboard: Vec<LeaderboardApiEntry>,
) -> SubmitEventProgressData {
    SubmitEventProgressData {
        name: event.name.clone(),
        is_doubles: event.is_doubles,
        score_delta: event.score_delta,
        rate_delta: event.rate_delta,
        top_score_points: event.top_score_points,
        prev_top_score_points: event.prev_top_score_points,
        total_passes: event.total_passes,
        current_ranking_point_total: event.current_ranking_point_total,
        previous_ranking_point_total: event.previous_ranking_point_total,
        current_song_point_total: event.current_song_point_total,
        previous_song_point_total: event.previous_song_point_total,
        current_ex_point_total: event.current_ex_point_total,
        previous_ex_point_total: event.previous_ex_point_total,
        current_point_total: event.current_point_total,
        previous_point_total: event.previous_point_total,
        leaderboard: leaderboard_entries_from_api(leaderboard),
        progress: event.progress.as_ref().map(submit_progress_from_api),
    }
}

pub fn leaderboard_entries_from_api(entries: Vec<LeaderboardApiEntry>) -> Vec<LeaderboardEntry> {
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        out.push(LeaderboardEntry {
            rank: entry.rank,
            name: entry.name,
            machine_tag: entry.machine_tag,
            score: entry.score,
            date: entry.date,
            is_rival: entry.is_rival,
            is_self: entry.is_self,
            is_fail: entry.is_fail,
        });
    }
    out
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

fn summary_page_text(
    progress: &ItlEventProgress,
    submit_progress: Option<&SubmitProgress>,
) -> String {
    match progress.kind {
        EventProgressKind::Itl => itl_summary_page_text(progress, submit_progress),
        EventProgressKind::Srpg => srpg_summary_page_text(progress),
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

pub fn arrowcloud_score_from_retrieve_fields(
    score: Option<f64>,
    grade: Option<&str>,
    date: Option<&str>,
    play_id: Option<i64>,
    is_fail: bool,
) -> Option<ArrowCloudScore> {
    let percent_0_100 = score?.clamp(0.0, 100.0);
    let server_grade = grade.and_then(ArrowCloudServerGrade::from_server_str);
    let played_at = date
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    Some(ArrowCloudScore {
        score_percent: percent_0_100 / 100.0,
        server_grade,
        played_at,
        play_id,
        is_fail,
    })
}

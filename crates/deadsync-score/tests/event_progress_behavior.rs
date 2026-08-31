use deadsync_score::{
    EventOverlayPage, EventProgress, EventProgressKind, EventStatImprovement, LeaderboardEntry,
    SubmitAchievement, SubmitAchievementReward, SubmitProgress, SubmitQuest, SubmitQuestReward,
    SubmitStatImprovement, event_progress_overlay_pages,
};

fn text(page: &EventOverlayPage) -> &str {
    match page {
        EventOverlayPage::Text(text) => text,
        EventOverlayPage::Leaderboard(_) => panic!("expected text page"),
    }
}

#[test]
fn summaries_preserve_all_line_rules_without_trailing_blank_lines() {
    let itl = EventProgress {
        kind: EventProgressKind::Itl,
        score_hundredths: 9_912,
        score_delta_hundredths: 34,
        current_points: 100,
        point_delta: 10,
        current_ranking_points: 200,
        ranking_delta: 50,
        current_song_points: 300,
        song_delta: 50,
        current_ex_points: 400,
        ex_delta: 50,
        current_total_points: 900,
        total_delta: 150,
        total_passes: 3,
        ..EventProgress::default()
    };
    let submit = SubmitProgress {
        stat_improvements: vec![
            SubmitStatImprovement {
                name: "clearType".into(),
                gained: 2,
                current: 5,
            },
            SubmitStatImprovement {
                name: "grade".into(),
                gained: 1,
                current: 1,
            },
            SubmitStatImprovement {
                name: "straßeLevel".into(),
                gained: 7,
                current: 12,
            },
            SubmitStatImprovement {
                name: "ignoredLevel".into(),
                gained: 0,
                current: 99,
            },
        ],
        ..SubmitProgress::default()
    };

    let pages = event_progress_overlay_pages(&itl, Some(&submit), &[]);
    assert_eq!(
        text(&pages[0]),
        "EX Score: 99.12% (+0.34%)\n\
         Points: 100 (+10)\n\n\
         Ranking Points: 200 (+50)\n\
         Song Points: 300 (+50)\n\
         EX Points: 400 (+50)\n\
         Total Points: 900 (+150)\n\n\
         You've passed the chart 3 times\n\n\
         Clear Type: FEC >>> FBFC\n\
         New Quad!\n\
         Straße Lvl: 12 (+7)"
    );

    let srpg = EventProgress {
        kind: EventProgressKind::Srpg,
        score_hundredths: 9_876,
        score_delta_hundredths: 9_876,
        rate_hundredths: Some(150),
        rate_delta_hundredths: Some(150),
        stat_improvements: vec![
            EventStatImprovement {
                name: "straße".into(),
                gained: 4,
                current: 8,
            },
            EventStatImprovement {
                name: "ignored".into(),
                gained: 0,
                current: 0,
            },
        ],
        skill_improvements: vec!["Reached Level 8".into(), String::new()],
        ..EventProgress::default()
    };
    let pages = event_progress_overlay_pages(&srpg, None, &[]);
    assert_eq!(
        text(&pages[0]),
        "Skill Improvements\n\n\
         98.76% (+98.76%) at\n\
         1.50x (+1.50x) rate\n\n\
         +4 STRASSE\n\n\
         Reached Level 8"
    );
}

#[test]
fn quest_grouping_preserves_first_kind_order_and_borrows_trimmed_text() {
    let quest = SubmitQuest {
        title: "  Unlock Route  ".into(),
        rewards: vec![
            SubmitQuestReward {
                reward_type: "pack".into(),
                description: "  First pack  ".into(),
            },
            SubmitQuestReward {
                reward_type: " PACK ".into(),
                description: "Second pack".into(),
            },
            SubmitQuestReward {
                reward_type: "ad-hoc".into(),
                description: "Loose reward".into(),
            },
            SubmitQuestReward {
                reward_type: "ignored".into(),
                description: "   ".into(),
            },
            SubmitQuestReward {
                reward_type: String::new(),
                description: "Mystery".into(),
            },
        ],
    };
    let submit = SubmitProgress {
        quests_completed: vec![quest],
        ..SubmitProgress::default()
    };
    let pages = event_progress_overlay_pages(&EventProgress::default(), Some(&submit), &[]);

    assert_eq!(
        text(&pages[1]),
        "Completed \"Unlock Route\"!\n\n\
         PACK:\n\
         First pack\n\
         Second pack\n\n\
         Loose reward\n\n\
         :\n\
         Mystery"
    );
}

#[test]
fn achievement_pages_and_page_order_match_the_published_contract() {
    let achievement = SubmitAchievement {
        title: "  Milestone  ".into(),
        rewards: vec![
            SubmitAchievementReward {
                tier: " 2 ".into(),
                requirements: vec![" Play one chart ".into(), "   ".into()],
                title_unlocked: " Champion ".into(),
            },
            SubmitAchievementReward {
                tier: "0".into(),
                requirements: vec!["Play another chart".into()],
                title_unlocked: String::new(),
            },
        ],
    };
    let submit = SubmitProgress {
        achievements_completed: vec![achievement],
        ..SubmitProgress::default()
    };
    let leaderboard = [LeaderboardEntry {
        rank: 1,
        name: "AAA".into(),
        machine_tag: Some("HOME".into()),
        score: 99.12,
        date: "2026-08-31".into(),
        is_rival: false,
        is_self: true,
        is_fail: false,
    }];
    let pages =
        event_progress_overlay_pages(&EventProgress::default(), Some(&submit), &leaderboard);

    assert_eq!(pages.len(), 3);
    assert_eq!(
        text(&pages[1]),
        "Completed the \"Milestone\" Achievement!\n\
         Tier 2\n\
         Play one chart\n\
         Unlocked the \"Champion\" Title!\n\n\
         Play another chart"
    );
    let EventOverlayPage::Leaderboard(entries) = &pages[2] else {
        panic!("leaderboard must remain the final page");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "AAA");
    assert_eq!(entries[0].score, 99.12);
}

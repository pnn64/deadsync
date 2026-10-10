//! Frozen baseline functions for paired regression tests and benchmarks.
use super::*;

pub(super) fn import_summary_message(
    summary: &crate::SimplyLoveItgImportSummary,
) -> ImportMessageState {
    let mut lines = Vec::new();
    lines.push(MessageLine::plain(
        tr_fmt(
            "Profiles",
            "ImportSummaryName",
            &[("name", &summary.display_name)],
        )
        .to_string(),
    ));

    // Scores: imported/total, amber when some were skipped.
    let scores_skipped =
        summary.charts_song_not_found + summary.charts_chart_not_found + summary.scores_unmapped;
    lines.push(if summary.scores_total == 0 {
        section_row(
            "ImportRowScores",
            tr("Profiles", "ImportStatNoneFound").to_string(),
            SectionStatus::Skipped,
        )
    } else {
        let status = ratio_status(&[
            ("done", &fmt_count(summary.scores_imported)),
            ("total", &fmt_count(summary.scores_total)),
        ]);
        let kind = if scores_skipped > 0 {
            SectionStatus::Partial
        } else {
            SectionStatus::Imported
        };
        section_row("ImportRowScores", status, kind)
    });

    // Favorites: matched/total, amber when some songs weren't found.
    lines.push(if summary.favorites_total == 0 {
        section_row(
            "ImportRowFavorites",
            tr("Profiles", "ImportStatNoneFound").to_string(),
            SectionStatus::Skipped,
        )
    } else {
        let status = ratio_status(&[
            ("done", &fmt_count(summary.favorites_imported)),
            ("total", &fmt_count(summary.favorites_total)),
        ]);
        let kind = if summary.favorites_imported < summary.favorites_total {
            SectionStatus::Partial
        } else {
            SectionStatus::Imported
        };
        section_row("ImportRowFavorites", status, kind)
    });

    // Player options (from Simply Love).
    lines.push(bool_row(
        "ImportRowPlayerOptions",
        summary.simply_love_options_imported,
        "ImportStatFromSimplyLove",
        "ImportStatDefaults",
    ));

    // GrooveStats / ArrowCloud credentials.
    lines.push(bool_row(
        "ImportRowGrooveStats",
        summary.groovestats_imported,
        "ImportStatLinked",
        "ImportStatNotSetUp",
    ));
    lines.push(bool_row(
        "ImportRowArrowCloud",
        summary.arrowcloud_imported,
        "ImportStatLinked",
        "ImportStatNotSetUp",
    ));

    // ITL event data.
    lines.push(if summary.itl_entries_imported > 0 {
        let status = tr_fmt(
            "Profiles",
            "ImportStatItlScores",
            &[("count", &fmt_count(summary.itl_entries_imported))],
        )
        .to_string();
        section_row("ImportRowItl", status, SectionStatus::Imported)
    } else {
        section_row(
            "ImportRowItl",
            tr("Profiles", "ImportStatNoneFound").to_string(),
            SectionStatus::Skipped,
        )
    });

    // Avatar.
    lines.push(bool_row(
        "ImportRowAvatar",
        summary.avatar_imported,
        "ImportStatImported",
        "ImportStatNoneFound",
    ));

    if summary.online_keys_imported() {
        lines.push(MessageLine::note(
            tr("Profiles", "ImportSummaryOnlineNudge").to_string(),
        ));
    }
    lines.push(MessageLine::note(
        tr("Profiles", "ImportSummaryExNote").to_string(),
    ));
    ImportMessageState::new(tr("Profiles", "ImportSummaryTitle"), lines)
}

pub(super) fn bool_row(label_key: &str, on: bool, done_key: &str, none_key: &str) -> MessageLine {
    if on {
        section_row(
            label_key,
            tr("Profiles", done_key).to_string(),
            SectionStatus::Imported,
        )
    } else {
        section_row(
            label_key,
            tr("Profiles", none_key).to_string(),
            SectionStatus::Skipped,
        )
    }
}

pub(super) fn ratio_status(args: &[(&str, &str)]) -> String {
    tr_fmt("Profiles", "ImportStatRatio", args).to_string()
}

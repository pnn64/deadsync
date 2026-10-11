use super::*;

#[inline(always)]
fn srpg_stat_lines(progress: &score_data::EventProgress) -> (Vec<String>, Vec<String>) {
    let srpg_stats = ["tp", "lp", "bb", "gold", "jp"];
    let show_qualifier_pair = progress.stat_improvements.len() >= 5;
    let mut qualifier = Vec::with_capacity(2);
    let mut stats = Vec::with_capacity(progress.stat_improvements.len());
    for improvement in &progress.stat_improvements {
        if improvement.gained == 0
            || !srpg_stats
                .iter()
                .any(|stat| improvement.name.eq_ignore_ascii_case(stat))
        {
            continue;
        }
        let line = format!(
            "+{} {}",
            improvement.gained,
            improvement.name.to_uppercase()
        );
        if show_qualifier_pair
            && (improvement.name.eq_ignore_ascii_case("tp")
                || improvement.name.eq_ignore_ascii_case("lp"))
        {
            qualifier.push(line);
        } else {
            stats.push(line);
        }
    }
    (qualifier, stats)
}

#[inline(always)]
fn srpg_overlay_stat_lines(progress: &score_data::EventProgress) -> Vec<String> {
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

#[inline(always)]
fn build_srpg_box_body(progress: &score_data::EventProgress) -> String {
    let mut body = format!(
        "Score: {} {}\n\
         Rate: {} {}\n",
        format_pct_hundredths(progress.score_hundredths),
        format_signed_pct_hundredths(progress.score_delta_hundredths),
        format_rate_hundredths(progress.rate_hundredths.unwrap_or(100)),
        format_signed_rate_hundredths(progress.rate_delta_hundredths.unwrap_or(0)),
    );
    let (qualifier, stats) = srpg_stat_lines(progress);
    if !qualifier.is_empty() || !stats.is_empty() {
        body.push('\n');
    }
    if !qualifier.is_empty() {
        body.push_str(qualifier.join(" ").as_str());
        body.push('\n');
    }
    for line in stats {
        body.push_str(line.as_str());
        body.push('\n');
    }
    body.trim_end().to_string()
}

#[inline(always)]
fn build_srpg_overlay_body(progress: &score_data::EventProgress) -> String {
    let mut text = format!(
        "Skill Improvements\n\n\
         {} {} at\n\
         {}x {} rate",
        format_pct_hundredths(progress.score_hundredths),
        format_signed_pct_hundredths(progress.score_delta_hundredths),
        format_rate_hundredths(progress.rate_hundredths.unwrap_or(100)),
        format_signed_rate_hundredths(progress.rate_delta_hundredths.unwrap_or(0)),
    );
    let stats = srpg_overlay_stat_lines(progress);
    if !stats.is_empty() {
        text.push_str("\n\n");
        text.push_str(stats.join("\n").as_str());
    }
    if !progress.skill_improvements.is_empty() {
        text.push_str("\n\n");
        text.push_str(progress.skill_improvements.join("\n").as_str());
    }
    text.trim_end().to_string()
}

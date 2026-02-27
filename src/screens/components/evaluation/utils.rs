use crate::core::space::screen_center_x;
use crate::game::profile;

const MONTH_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[inline(always)]
pub(super) fn pane_origin_x(controller: profile::PlayerSide) -> f32 {
    match controller {
        profile::PlayerSide::P1 => screen_center_x() - 155.0,
        profile::PlayerSide::P2 => screen_center_x() + 155.0,
    }
}

pub(super) fn format_machine_record_date(date: &str) -> String {
    let trimmed = date.trim();
    if trimmed.is_empty() {
        return "----------".to_string();
    }

    let ymd = trimmed.split_once(' ').map_or(trimmed, |(d, _)| d);
    let ymd = ymd.split_once('T').map_or(ymd, |(d, _)| d);
    let mut parts = ymd.split('-');
    let (Some(year), Some(month), Some(day)) = (parts.next(), parts.next(), parts.next()) else {
        return trimmed.to_string();
    };

    let Some(month_idx) = month
        .parse::<usize>()
        .ok()
        .and_then(|m| m.checked_sub(1))
        .filter(|m| *m < MONTH_ABBR.len())
    else {
        return trimmed.to_string();
    };
    let Some(day_num) = day.parse::<u32>().ok().filter(|d| *d > 0) else {
        return trimmed.to_string();
    };

    format!("{} {}, {}", MONTH_ABBR[month_idx], day_num, year)
}

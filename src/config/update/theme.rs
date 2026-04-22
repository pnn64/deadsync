use super::*;

pub fn update_select_music_breakdown_style(style: BreakdownStyle) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_breakdown_style == style {
            return;
        }
        cfg.select_music_breakdown_style = style;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_breakdown(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_breakdown == enabled {
            return;
        }
        cfg.show_select_music_breakdown = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_banners(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_banners == enabled {
            return;
        }
        cfg.show_select_music_banners = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_video_banners(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_video_banners == enabled {
            return;
        }
        cfg.show_select_music_video_banners = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_cdtitles(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_cdtitles == enabled {
            return;
        }
        cfg.show_select_music_cdtitles = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_music_wheel_grades(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_music_wheel_grades == enabled {
            return;
        }
        cfg.show_music_wheel_grades = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_music_wheel_lamps(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_music_wheel_lamps == enabled {
            return;
        }
        cfg.show_music_wheel_lamps = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_itl_chart_rank(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_itl_chart_rank == enabled {
            return;
        }
        cfg.show_select_music_itl_chart_rank = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_itl_wheel_mode(mode: SelectMusicItlWheelMode) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_itl_wheel_mode == mode {
            return;
        }
        cfg.select_music_itl_wheel_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_select_music_wheel_style(style: SelectMusicWheelStyle) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_wheel_style == style {
            return;
        }
        cfg.select_music_wheel_style = style;
    }
    save_without_keymaps();
}

pub fn update_select_music_new_pack_mode(mode: NewPackMode) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_new_pack_mode == mode {
            return;
        }
        cfg.select_music_new_pack_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_previews(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_previews == enabled {
            return;
        }
        cfg.show_select_music_previews = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_preview_marker(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_preview_marker == enabled {
            return;
        }
        cfg.show_select_music_preview_marker = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_preview_loop(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_preview_loop == enabled {
            return;
        }
        cfg.select_music_preview_loop = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_pattern_info_mode(mode: SelectMusicPatternInfoMode) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_pattern_info_mode == mode {
            return;
        }
        cfg.select_music_pattern_info_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_gameplay_timer(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_gameplay_timer == enabled {
            return;
        }
        cfg.show_select_music_gameplay_timer = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_scorebox(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_scorebox == enabled {
            return;
        }
        cfg.show_select_music_scorebox = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_placement(mode: SelectMusicScoreboxPlacement) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_placement == mode {
            return;
        }
        cfg.select_music_scorebox_placement = mode;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_cycle_itg(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_cycle_itg == enabled {
            return;
        }
        cfg.select_music_scorebox_cycle_itg = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_cycle_ex(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_cycle_ex == enabled {
            return;
        }
        cfg.select_music_scorebox_cycle_ex = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_cycle_hard_ex(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_cycle_hard_ex == enabled {
            return;
        }
        cfg.select_music_scorebox_cycle_hard_ex = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_cycle_tournaments(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_cycle_tournaments == enabled {
            return;
        }
        cfg.select_music_scorebox_cycle_tournaments = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_chart_info_peak_nps(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_chart_info_peak_nps == enabled {
            return;
        }
        cfg.select_music_chart_info_peak_nps = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_chart_info_matrix_rating(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_chart_info_matrix_rating == enabled {
            return;
        }
        cfg.select_music_chart_info_matrix_rating = enabled;
    }
    save_without_keymaps();
}

pub fn update_auto_screenshot_eval(mask: u8) {
    {
        let mut cfg = lock_config();
        if cfg.auto_screenshot_eval == mask {
            return;
        }
        cfg.auto_screenshot_eval = mask;
    }
    save_without_keymaps();
}

pub fn update_show_random_courses(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_random_courses == enabled {
            return;
        }
        cfg.show_random_courses = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_most_played_courses(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_most_played_courses == enabled {
            return;
        }
        cfg.show_most_played_courses = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_course_individual_scores(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_course_individual_scores == enabled {
            return;
        }
        cfg.show_course_individual_scores = enabled;
    }
    save_without_keymaps();
}

pub fn update_autosubmit_course_scores_individually(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.autosubmit_course_scores_individually == enabled {
            return;
        }
        cfg.autosubmit_course_scores_individually = enabled;
    }
    save_without_keymaps();
}

pub fn update_zmod_rating_box_text(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.zmod_rating_box_text == enabled {
            return;
        }
        cfg.zmod_rating_box_text = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_bpm_decimal(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_bpm_decimal == enabled {
            return;
        }
        cfg.show_bpm_decimal = enabled;
    }
    save_without_keymaps();
}

pub fn update_default_fail_type(fail_type: DefaultFailType) {
    {
        let mut cfg = lock_config();
        if cfg.default_fail_type == fail_type {
            return;
        }
        cfg.default_fail_type = fail_type;
    }
    save_without_keymaps();
}

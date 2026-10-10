pub(super) fn build_init_data(init_view: &SelectCourseInitView) -> InitData {
    let translated_titles = init_view.translated_titles;
    let target_chart_type = init_view.context.play_style.chart_type();
    let (by_group_song, by_song, songs_by_group, all_songs, song_play_counts) =
        build_song_lookup(&init_view.song_packs, &init_view.played_chart_counts);
    let song_grade_counts = build_song_grade_counts(
        &init_view.song_packs,
        &init_view.chart_grades,
        target_chart_type,
    );

    let mut grouped: HashMap<String, Vec<Arc<CourseMeta>>> = HashMap::new();
    let mut course_meta_by_path: HashMap<PathBuf, Arc<CourseMeta>> = HashMap::new();

    for (path, course) in &init_view.courses {
        let course_type = course::course_type(course);
        let mut total_seconds = 0i32;
        let mut min_bpm = None;
        let mut max_bpm = None;
        let mut selected_song_keys = Vec::with_capacity(course.entries.len());
        let mut has_random_entries = false;
        let mut has_most_played_entries = false;
        let random_seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0_u64, |d| d.as_nanos() as u64);

        for (entry_idx, entry) in course.entries.iter().enumerate() {
            if matches!(
                &entry.song,
                CourseSong::RandomAny
                    | CourseSong::RandomWithinGroup { .. }
                    | CourseSong::Select(_)
            ) {
                has_random_entries = true;
            }
            if matches!(
                &entry.song,
                CourseSong::SortPick {
                    sort: SongSort::MostPlays,
                    ..
                }
            ) || matches!(
                &entry.song,
                CourseSong::Select(select) if select.sort == Some(SongSort::MostPlays)
            ) {
                has_most_played_entries = true;
            }

            let resolved = resolve_course_stage(
                path,
                entry_idx,
                random_seed,
                entry,
                &by_group_song,
                &by_song,
                &all_songs,
                &songs_by_group,
                &song_play_counts,
                &song_grade_counts,
                course_type,
                &selected_song_keys,
                target_chart_type,
                Difficulty::Medium,
            );

            if let Some(stage) = resolved.as_ref() {
                let song_data = &stage.song;
                selected_song_keys.push(song_unique_key(song_data));
                let len = if song_data.music_length_seconds > 0.0 {
                    song_data.music_length_seconds.round() as i32
                } else {
                    song_data.total_length_seconds.max(0)
                };
                total_seconds = total_seconds.saturating_add(len.max(0));
                push_song_bpm_range(&mut min_bpm, &mut max_bpm, song_data);
            }
        }

        let preferred_default_idx = course_difficulty_from_meters(course)
            .and_then(|(difficulty_name, _)| {
                COURSE_RATING_ORDER.iter().position(|diff| {
                    course::difficulty_label(*diff).eq_ignore_ascii_case(difficulty_name)
                })
            })
            .unwrap_or(Difficulty::Medium as usize);
        let preferred_default_diff = COURSE_RATING_ORDER[preferred_default_idx];
        let mut available_course_diffs: Vec<Difficulty> = COURSE_RATING_ORDER
            .iter()
            .copied()
            .filter(|diff| course_meter(course, *diff).is_some_and(|meter| meter >= 0))
            .collect();
        if has_random_entries && available_course_diffs.len() <= 1 {
            available_course_diffs = COURSE_RATING_ORDER.to_vec();
        }
        if available_course_diffs.is_empty() {
            available_course_diffs.push(preferred_default_diff);
        }

        let mut ratings: Vec<Option<CourseRatingMeta>> = vec![None; COURSE_RATING_ORDER.len()];
        for course_diff in available_course_diffs {
            let mut entries = Vec::with_capacity(course.entries.len());
            let mut runtime_stages = Vec::with_capacity(course.entries.len());
            let mut totals = CourseTotals::default();
            let mut rated_entry_count = 0usize;
            let mut meter_sum = 0u32;
            let mut meter_count = 0usize;
            let mut rating_song_keys = Vec::with_capacity(course.entries.len());
            let mut rating_total_seconds = 0i32;
            let mut rating_min_bpm = None;
            let mut rating_max_bpm = None;

            for (entry_idx, entry) in course.entries.iter().enumerate() {
                let Some(stage) = resolve_course_stage(
                    path,
                    entry_idx,
                    random_seed,
                    entry,
                    &by_group_song,
                    &by_song,
                    &all_songs,
                    &songs_by_group,
                    &song_play_counts,
                    &song_grade_counts,
                    course_type,
                    &rating_song_keys,
                    target_chart_type,
                    course_diff,
                ) else {
                    continue;
                };
                let song_data = &stage.song;
                let Some(chart) = song_data.charts.get(stage.chart_index) else {
                    continue;
                };
                rating_song_keys.push(song_unique_key(song_data));
                let len = if song_data.music_length_seconds > 0.0 {
                    song_data.music_length_seconds.round() as i32
                } else {
                    song_data.total_length_seconds.max(0)
                };
                rating_total_seconds = rating_total_seconds.saturating_add(len.max(0));
                push_song_bpm_range(&mut rating_min_bpm, &mut rating_max_bpm, song_data);
                runtime_stages.push(CourseStagePlan {
                    song: song_data.clone(),
                    chart_hash: chart.short_hash.clone(),
                    modifiers: stage.modifiers,
                    gain_seconds: stage.gain_seconds,
                    gain_lives: stage.gain_lives,
                });
                add_chart_totals(&mut totals, chart);
                rated_entry_count = rated_entry_count.saturating_add(1);
                meter_sum = meter_sum.saturating_add(chart.meter);
                meter_count = meter_count.saturating_add(1);
                let entry_index = entries.len();
                entries.push(CourseSongEntry::new(
                    song_data.display_full_title(translated_titles),
                    chart.difficulty.to_ascii_lowercase(),
                    chart.meter,
                    entry_index,
                    chart_step_artist(chart),
                ));
            }

            let explicit_meter = course_meter(course, course_diff)
                .filter(|v| *v >= 0)
                .map(|v| v as u32);
            if rated_entry_count == 0
                && explicit_meter.is_none()
                && course_diff != Difficulty::Medium
            {
                continue;
            }

            let course_meter = explicit_meter.or_else(|| {
                if meter_count > 0 {
                    Some((meter_sum as f32 / meter_count as f32).round() as u32)
                } else {
                    None
                }
            });
            let course_difficulty_name = course::difficulty_label(course_diff).to_string();
            let course_stepchart_label =
                course_stepchart_label(course_difficulty_name.as_str(), course_meter);
            let course_meter_text =
                course_meter.map_or_else(unknown_text, |meter| Arc::<str>::from(meter.to_string()));
            let entry_count_text = Arc::<str>::from(entries.len().to_string());
            let stats_text = CourseStatsText::new(&totals, rated_entry_count > 0);

            ratings[course_diff as usize] = Some(CourseRatingMeta {
                course_difficulty: course_diff,
                entries,
                stats_text,
                course_difficulty_name,
                course_stepchart_label,
                course_meter,
                course_meter_text,
                entry_count_text,
                min_bpm: rating_min_bpm,
                max_bpm: rating_max_bpm,
                total_length_seconds: rating_total_seconds.max(0),
                runtime_stages,
            });
        }

        let group_name = course_group_name(path);
        let default_rating_index =
            nearest_filled_slot(&ratings, preferred_default_idx).unwrap_or(preferred_default_idx);
        let (meta_min_bpm, meta_max_bpm, meta_total_length_seconds) = ratings
            .get(default_rating_index)
            .and_then(Option::as_ref)
            .map(|rating| {
                (
                    rating.min_bpm,
                    rating.max_bpm,
                    rating.total_length_seconds.max(0),
                )
            })
            .unwrap_or_else(|| (min_bpm, max_bpm, total_seconds.max(0)));
        let meta = Arc::new(CourseMeta {
            source: course.clone(),
            path: path.clone(),
            score_hash: course_score_hash(path),
            name: course_name(path, course),
            scripter: course.scripter.clone(),
            description: Arc::from(course.description.as_str()),
            banner_path: course::resolve_course_banner_path(path, &course.banner),
            ratings,
            default_rating_index,
            min_bpm: meta_min_bpm,
            max_bpm: meta_max_bpm,
            total_length_seconds: meta_total_length_seconds,
            has_random_entries,
            has_most_played_entries,
            course_type: match course_type {
                course::CourseType::Nonstop => CourseTypeView::Nonstop,
                course::CourseType::Oni => CourseTypeView::Oni,
                course::CourseType::Endless => CourseTypeView::Endless,
                course::CourseType::Survival => CourseTypeView::Survival,
            },
            lives: course.lives,
        });

        grouped.entry(group_name).or_default().push(meta.clone());
        course_meta_by_path.insert(meta.path.clone(), meta);
    }

    let mut all_courses: Vec<Arc<CourseMeta>> = grouped.into_values().flatten().collect();
    all_courses.sort_by_cached_key(|c| c.name.to_ascii_lowercase());

    let mut all_entries = Vec::with_capacity(all_courses.len());
    let mut course_text_color_overrides = HashMap::with_capacity(all_courses.len());
    for meta in all_courses {
        let song_stub = Arc::new(make_course_song(&meta));
        if meta.has_random_entries {
            course_text_color_overrides.insert(
                Arc::as_ptr(&song_stub) as usize,
                COURSE_WHEEL_RANDOM_TEXT_COLOR,
            );
        }
        all_entries.push(MusicWheelEntry::Song(song_stub));
    }

    InitData {
        all_entries,
        course_meta_by_path,
        course_text_color_overrides,
        resolver: CourseResolver {
            by_group_song,
            by_song,
            songs_by_group,
            all_songs,
            song_play_counts,
            song_grade_counts,
            target_chart_type: target_chart_type.to_string(),
        },
    }
}

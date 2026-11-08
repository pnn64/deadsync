// Frozen from main 16a06a2619a6cdcc34c647603d5dfa186e2f3e73; visibility only.

pub(super) fn load_course_paths_with_progress<F>(
    course_paths: Vec<PathBuf>,
    progress_root: &Path,
    song_roots: &[PathBuf],
    total_song_count: usize,
    mut progress: Option<&mut F>,
) -> CourseLoadReport
where
    F: FnMut(usize, usize, &str, &str),
{
    let total_courses = course_paths.len();
    let mut courses = Vec::with_capacity(total_courses);
    let mut failures = Vec::new();
    let mut group_dirs = HashMap::new();
    let mut courses_done = 0usize;
    report_load_progress(&mut progress, 0, total_courses, "", "");

    for course_path in course_paths {
        let (group_display, course_display) = course_progress_names(&course_path, progress_root);
        let group_display = group_display.to_owned();
        let course_display = course_display.to_owned();
        let mut report_done = || {
            courses_done = courses_done.saturating_add(1);
            report_load_progress(
                &mut progress,
                courses_done,
                total_courses,
                &group_display,
                &course_display,
            );
        };

        let course = match parse_course_file(&course_path) {
            Ok(course) => course,
            Err(message) => {
                failures.push(CourseLoadFailure {
                    path: course_path,
                    message,
                });
                report_done();
                continue;
            }
        };

        match validate_course_refs(&course, song_roots, &mut group_dirs, total_song_count) {
            Ok(()) => courses.push((course_path, course)),
            Err(error) => failures.push(CourseLoadFailure {
                path: course_path,
                message: error.message,
            }),
        }
        report_done();
    }

    CourseLoadReport { courses, failures }
}

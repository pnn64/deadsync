use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::Mutex;

pub type CourseData = (PathBuf, rssp::course::CourseFile);

static COURSE_CACHE: Lazy<Mutex<Vec<CourseData>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub fn get_course_cache() -> std::sync::MutexGuard<'static, Vec<CourseData>> {
    COURSE_CACHE.lock().unwrap()
}

pub(super) fn set_course_cache(courses: Vec<CourseData>) {
    *COURSE_CACHE.lock().unwrap() = courses;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CourseType {
    #[default]
    Nonstop,
    Oni,
    Endless,
    Survival,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum NoteType {
    Tap,
    Hold,
    Roll,
    Mine,
    Lift,
    Fake,
}

#[cfg(test)]
mod tests {
    use super::NoteType;

    #[test]
    fn note_type_equality_is_value_based() {
        assert_eq!(NoteType::Tap, NoteType::Tap);
        assert_ne!(NoteType::Hold, NoteType::Roll);
    }
}

use super::*;


#[test]
fn owned_user_ids_preserve_unicode_whitespace_case_and_duplicate_semantics() {
    let result = user_context_from_api(ArrowCloudUserApiUser {
        id: " \t ".into(),
        rival_user_ids: vec![" a ".into(), "a".into(), "A".into(), " ".into()],
    });
    assert!(result.self_user_id.is_none());
    assert_eq!(
        result.rival_user_ids,
        HashSet::from(["a".into(), "A".into()])
    );
}

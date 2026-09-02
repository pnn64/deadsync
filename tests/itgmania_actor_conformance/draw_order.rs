use super::support::fixture;
use deadsync_theme_simply_love::screens::gameplay::actor_conformance::stable_draw_order;

#[test]
fn draw_order_sort_matches_itgmania_and_preserves_equal_input_order() {
    let oracle = fixture("draw-order");
    let record = &oracle["post_draw_order"][0];
    let sample = &oracle["samples"][0];
    let input = record["input"]
        .as_array()
        .expect("draw input")
        .iter()
        .map(|name| {
            let name = name.as_str().expect("actor name");
            let actor = sample["actors"]
                .as_array()
                .expect("sample actors")
                .iter()
                .find(|actor| actor["name"].as_str() == Some(name))
                .expect("ordered actor");
            (
                name.to_owned(),
                actor["draw_order"].as_i64().expect("draw order") as i32,
            )
        })
        .collect::<Vec<_>>();
    let expected = record["post"]
        .as_array()
        .expect("post draw order")
        .iter()
        .map(|name| name.as_str().expect("post actor name").to_owned())
        .collect::<Vec<_>>();

    assert_eq!(stable_draw_order(&input), expected);
}

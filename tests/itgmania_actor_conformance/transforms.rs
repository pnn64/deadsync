use super::support::{actor, assert_array_ulp, assert_matrix, f32_array, fixture, matrix};
use deadsync_theme_simply_love::screens::gameplay::actor_conformance::{
    actor_matrix, multiply_matrices, project_world, sprite_matrix, view_projection,
};

#[test]
fn root_actor_translation_rotation_and_zoom_match_itgmania() {
    let oracle = fixture("transform-projection");
    let native = actor(&oracle["samples"][0], "root");
    let actual = actor_matrix(
        [320.0, 240.0, 0.0],
        [0.0, 0.0, 8.0],
        [1.1, 0.9, 1.0],
        [0.0, 0.0],
    );
    assert_matrix(actual, matrix(&native["local_matrix"]), "root local matrix");
}

#[test]
fn projected_vertex_clip_coordinates_match_itgmania() {
    let oracle = fixture("transform-projection");
    let sample = &oracle["samples"][0];
    let view_projection = view_projection([640, 480], 45.0, [280.0, 210.0]);
    let projected = actor(sample, "projected");
    for (index, vertex) in projected["draws"][0]["vertices"]
        .as_array()
        .expect("projected draw vertices")
        .iter()
        .enumerate()
    {
        let world = f32_array(&vertex["world"]);
        let expected = f32_array(&vertex["clip"]);
        assert_array_ulp(
            project_world(view_projection, world),
            expected,
            64,
            &format!("projected clip vertex {index}"),
        );
    }
}

#[test]
fn sprite_alignment_basezoom_and_skew_match_itgmania() {
    let oracle = fixture("transform-projection");
    let native = actor(&oracle["samples"][0], "projected");
    let actual = sprite_matrix(
        [35.0, -20.0, 65.0],
        [13.0, 17.0, 19.0],
        [4.0, 7.0, 11.0],
        [1.1, 0.85, 1.0],
        [1.2, 0.7, 1.0],
        [180.0, 90.0],
        [0.0, 1.0],
        [0.18, -0.11],
    );
    assert_matrix(
        actual,
        matrix(&native["local_matrix"]),
        "projected sprite local matrix",
    );
}

#[test]
fn projected_sprite_inherited_world_matrix_matches_itgmania() {
    let oracle = fixture("transform-projection");
    let sample = &oracle["samples"][0];
    let parent = matrix(&actor(sample, "nested")["world_matrix"]);
    let sprite = sprite_matrix(
        [35.0, -20.0, 65.0],
        [13.0, 17.0, 19.0],
        [4.0, 7.0, 11.0],
        [1.1, 0.85, 1.0],
        [1.2, 0.7, 1.0],
        [180.0, 90.0],
        [0.0, 1.0],
        [0.18, -0.11],
    );
    assert_matrix(
        multiply_matrices(parent, sprite),
        matrix(&actor(sample, "projected")["world_matrix"]),
        "projected sprite world matrix",
    );
}

#[test]
fn nested_actor_xyz_rotation_order_matches_itgmania() {
    let oracle = fixture("transform-projection");
    let native = actor(&oracle["samples"][0], "nested");
    let actual = actor_matrix(
        [-75.0, 40.0, 30.0],
        [12.0, -18.0, 22.0],
        [0.8, 1.25, 1.0],
        [0.0, 0.0],
    );
    assert_matrix(
        actual,
        matrix(&native["local_matrix"]),
        "nested local matrix",
    );
}

#[test]
fn inherited_actor_transform_matches_itgmania() {
    let oracle = fixture("transform-projection");
    let sample = &oracle["samples"][0];
    let root = actor_matrix(
        [320.0, 240.0, 0.0],
        [0.0, 0.0, 8.0],
        [1.1, 0.9, 1.0],
        [0.0, 0.0],
    );
    let nested = actor_matrix(
        [-75.0, 40.0, 30.0],
        [12.0, -18.0, 22.0],
        [0.8, 1.25, 1.0],
        [0.0, 0.0],
    );
    assert_matrix(
        multiply_matrices(root, nested),
        matrix(&actor(sample, "nested")["world_matrix"]),
        "nested world matrix",
    );
}

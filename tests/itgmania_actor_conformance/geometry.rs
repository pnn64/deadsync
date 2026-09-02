use super::support::{actor, assert_array_ulp, f32_array, fixture};
use deadsync_song_lua::SongLuaOverlayState;
use deadsync_theme_simply_love::screens::gameplay::actor_conformance::{
    SpriteVertex, crop_fade_vertices,
};

fn fixture_state() -> SongLuaOverlayState {
    SongLuaOverlayState {
        x: 320.0,
        y: 240.0,
        cropleft: 0.1,
        cropright: 0.2,
        croptop: 0.15,
        cropbottom: 0.05,
        fadeleft: 0.08,
        faderight: 0.16,
        fadetop: 0.12,
        fadebottom: 0.1,
        vertex_colors: Some([
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 0.8],
            [0.0, 0.0, 1.0, 0.6],
            [1.0, 1.0, 0.0, 0.4],
        ]),
        glow: [0.25, 0.5, 1.0, 0.35],
        custom_texture_rect: Some([0.125, 0.25, 0.875, 0.75]),
        ..SongLuaOverlayState::default()
    }
}

fn canonical(mut vertices: Vec<SpriteVertex>) -> Vec<SpriteVertex> {
    vertices.sort_by(|a, b| {
        a.position[1]
            .total_cmp(&b.position[1])
            .then(a.position[0].total_cmp(&b.position[0]))
            .then(a.uv[1].total_cmp(&b.uv[1]))
            .then(a.uv[0].total_cmp(&b.uv[0]))
            .then(a.color[0].total_cmp(&b.color[0]))
            .then(a.color[1].total_cmp(&b.color[1]))
            .then(a.color[2].total_cmp(&b.color[2]))
            .then(a.color[3].total_cmp(&b.color[3]))
    });
    vertices
}

fn native_vertices() -> Vec<SpriteVertex> {
    let oracle = fixture("crop-fade-color");
    let native = actor(&oracle["samples"][0], "gradient");
    let vertices = native["draws"]
        .as_array()
        .expect("native draws")
        .iter()
        .filter(|draw| draw["texture_mode"].as_str() == Some("modulate"))
        .flat_map(|draw| {
            let vertices = draw["vertices"].as_array().expect("draw vertices");
            // Native emits TL, BL, BR, TR quads; DeadSync emits triangles.
            [0, 3, 2, 0, 2, 1].map(|index| &vertices[index])
        })
        .map(|vertex| {
            let color = f32_array::<4>(&vertex["color"]).map(|channel| channel / 255.0);
            SpriteVertex {
                position: f32_array(&vertex["screen"]),
                uv: f32_array(&vertex["uv"]),
                color,
            }
        })
        .collect();
    canonical(vertices)
}

#[test]
fn crop_and_fade_emit_same_cross_topology_as_itgmania() {
    let oracle = fixture("crop-fade-color");
    let native = actor(&oracle["samples"][0], "gradient");
    let native_quads = native["draws"]
        .as_array()
        .expect("native draws")
        .iter()
        .filter(|draw| draw["texture_mode"].as_str() == Some("modulate"))
        .count();
    let actual = crop_fade_vertices(fixture_state(), [240.0, 160.0]);
    assert_eq!(
        actual.len(),
        native_quads * 6,
        "two triangles per native quad"
    );
}

#[test]
fn crop_and_fade_vertex_positions_and_uvs_match_itgmania() {
    let actual = canonical(crop_fade_vertices(fixture_state(), [240.0, 160.0]));
    let expected = native_vertices();
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_array_ulp(
            actual.position,
            expected.position,
            64,
            &format!("position {index}"),
        );
        assert_array_ulp(actual.uv, expected.uv, 32, &format!("uv {index}"));
    }
}

#[test]
fn crop_and_fade_vertex_colors_match_itgmania_quantization() {
    let actual = canonical(crop_fade_vertices(fixture_state(), [240.0, 160.0]));
    let expected = native_vertices();
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        for channel in 0..4 {
            assert_eq!(
                (actual.color[channel].clamp(0.0, 1.0) * 255.0).round() as u8,
                (expected.color[channel] * 255.0).round() as u8,
                "color {index}[{channel}]: {:?} != {:?}",
                actual.color,
                expected.color,
            );
        }
    }
}

#[test]
fn glow_color_matches_itgmania_byte_conversion() {
    let oracle = fixture("crop-fade-color");
    let native = actor(&oracle["samples"][0], "gradient");
    let expected =
        f32_array::<4>(&native["draws"][1]["vertices"][0]["color"]).map(|channel| channel / 255.0);
    let actual = fixture_state().glow;
    for channel in 0..4 {
        assert_eq!(
            (actual[channel].clamp(0.0, 1.0) * 255.0).round() as u8,
            (expected[channel] * 255.0).round() as u8,
        );
    }
}

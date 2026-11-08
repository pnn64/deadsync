// Frozen from b86cc66df; only test visibility is widened.
use super::*;

pub(super) fn actor_type_is(actor: &Table, expected: &str) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<LuaFieldText<32>>>("__songlua_actor_type")?
        .is_some_and(|kind| kind.as_str().eq_ignore_ascii_case(expected)
            || (expected.eq_ignore_ascii_case("Sprite") && kind.as_str().eq_ignore_ascii_case("Banner"))))
}

pub(super) fn read_actor_multi_vertex_mesh(
    actor: &Table,
) -> Result<Option<Arc<[SongLuaOverlayMeshVertex]>>, String> {
    let source: Arc<[SongLuaActorMultiVertexPoint]> = match actor
        .get::<Value>("__songlua_vertices")
        .map_err(|err| err.to_string())?
    {
        Value::Nil => return Ok(None),
        Value::UserData(points) => {
            let points = points
                .borrow::<Arc<[SongLuaActorMultiVertexPoint]>>()
                .map_err(|err| err.to_string())?;
            Arc::clone(&points)
        }
        // Retain compatibility with compiler fixtures that supply raw tables.
        Value::Table(vertices) => read_actor_multi_vertex_points(&vertices)?.into(),
        _ => return Err("invalid ActorMultiVertex vertex buffer".into()),
    };
    let Some((start, end)) = read_actor_multi_vertex_range(actor, source.len())? else {
        return Ok(None);
    };
    let draw_mode = actor
        .get::<Option<String>>("__songlua_draw_state_mode")
        .map_err(|err| err.to_string())?
        .unwrap_or_else(|| "DrawMode_Triangles".to_string());
    let line_width = actor
        .get::<Option<f32>>("__songlua_line_width")
        .map_err(|err| err.to_string())?
        .unwrap_or(1.0)
        .max(0.0);
    let mesh = actor_multi_vertex_triangles(&source[start..end], draw_mode.as_str(), line_width);
    if mesh.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Arc::from(mesh.into_boxed_slice())))
    }
}

pub(super) fn actor_multi_vertex_triangles(
    vertices: &[SongLuaActorMultiVertexPoint],
    draw_mode: &str,
    line_width: f32,
) -> Vec<SongLuaOverlayMeshVertex> {
    if draw_mode.eq_ignore_ascii_case("DrawMode_Quads") {
        return actor_multi_vertex_quads(vertices);
    }
    if draw_mode.eq_ignore_ascii_case("DrawMode_QuadStrip") {
        return actor_multi_vertex_quad_strip(vertices);
    }
    if draw_mode.eq_ignore_ascii_case("DrawMode_LineStrip") {
        return actor_multi_vertex_line_strip(vertices, line_width);
    }
    if draw_mode.eq_ignore_ascii_case("DrawMode_Fan") {
        return actor_multi_vertex_fan(vertices);
    }
    actor_multi_vertex_triangle_list(vertices)
}

fn actor_multi_vertex_triangle_list(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    let mut out = Vec::with_capacity(vertices.len() / 3 * 3);
    for chunk in vertices.as_chunks::<3>().0 {
        push_actor_multi_vertex_triangle(&mut out, chunk[0], chunk[1], chunk[2]);
    }
    out
}

fn actor_multi_vertex_quads(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    let mut out = Vec::with_capacity(vertices.len() / 4 * 6);
    for chunk in vertices.as_chunks::<4>().0 {
        push_actor_multi_vertex_quad(&mut out, chunk[0], chunk[1], chunk[2], chunk[3]);
    }
    out
}

fn actor_multi_vertex_quad_strip(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    let mut out = Vec::with_capacity(vertices.len().saturating_sub(2) / 2 * 6);
    let mut index = 0usize;
    while index + 3 < vertices.len() {
        push_actor_multi_vertex_quad(
            &mut out,
            vertices[index],
            vertices[index + 1],
            vertices[index + 3],
            vertices[index + 2],
        );
        index += 2;
    }
    out
}

fn actor_multi_vertex_fan(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    if vertices.len() < 3 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(vertices.len().saturating_sub(2) * 3);
    for index in 1..vertices.len() - 1 {
        push_actor_multi_vertex_triangle(
            &mut out,
            vertices[0],
            vertices[index],
            vertices[index + 1],
        );
    }
    out
}

fn push_actor_multi_vertex_quad(
    out: &mut Vec<SongLuaOverlayMeshVertex>,
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
    c: SongLuaActorMultiVertexPoint,
    d: SongLuaActorMultiVertexPoint,
) {
    push_actor_multi_vertex_triangle(out, a, b, c);
    push_actor_multi_vertex_triangle(out, c, d, a);
}

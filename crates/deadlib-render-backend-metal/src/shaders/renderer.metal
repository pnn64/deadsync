#include <metal_stdlib>
using namespace metal;

struct SpriteInstance {
    packed_float4 center;
    packed_float2 size;
    packed_float2 rot_sin_cos;
    packed_float4 tint;
    packed_float2 uv_scale;
    packed_float2 uv_offset;
    packed_float2 local_offset;
    packed_float2 local_offset_rot_sin_cos;
    packed_float4 edge_fade;
    float texture_mask;
};

struct SpriteOut {
    float4 pos [[position]];
    float2 uv;
    float2 quad_uv;
    float4 tint;
    float4 edge_fade;
    float texture_mask;
};

vertex SpriteOut sprite_vertex(
    uint vertex_id [[vertex_id]],
    uint instance_id [[instance_id]],
    const device SpriteInstance *instances [[buffer(0)]],
    constant float4x4 &proj [[buffer(1)]])
{
    constexpr float2 positions[6] = {
        float2(-0.5, -0.5), float2(0.5, -0.5), float2(0.5, 0.5),
        float2(0.5, 0.5), float2(-0.5, 0.5), float2(-0.5, -0.5),
    };
    constexpr float2 uvs[6] = {
        float2(0.0, 1.0), float2(1.0, 1.0), float2(1.0, 0.0),
        float2(1.0, 0.0), float2(0.0, 0.0), float2(0.0, 1.0),
    };
    SpriteInstance inst = instances[instance_id];
    float2 size = float2(inst.size);
    float2 rot = float2(inst.rot_sin_cos);
    float2 local = positions[vertex_id] * size;
    float2 rotated = float2(
        rot.y * local.x - rot.x * local.y,
        rot.x * local.x + rot.y * local.y);
    float2 offset = float2(inst.local_offset);
    float2 offset_rot = float2(inst.local_offset_rot_sin_cos);
    float2 offset_world = float2(
        offset_rot.y * offset.x - offset_rot.x * offset.y,
        offset_rot.x * offset.x + offset_rot.y * offset.y);
    float4 center = float4(inst.center);

    SpriteOut out;
    out.pos = proj * float4(center.xy + rotated + offset_world, center.z, 1.0);
    // Engine cameras use [-w, w] depth; Metal clips to [0, w].
    out.pos.z = (out.pos.z + out.pos.w) * 0.5;
    out.uv = uvs[vertex_id] * float2(inst.uv_scale) + float2(inst.uv_offset);
    out.quad_uv = uvs[vertex_id];
    out.tint = float4(inst.tint);
    out.edge_fade = float4(inst.edge_fade);
    out.texture_mask = inst.texture_mask;
    return out;
}

static float edge_factor(float t, float feather_l, float feather_r)
{
    float l = feather_l > 0.0 ? clamp(t / feather_l, 0.0, 1.0) : 1.0;
    float r = feather_r > 0.0 ? clamp((1.0 - t) / feather_r, 0.0, 1.0) : 1.0;
    return min(l, r);
}

fragment float4 sprite_fragment(
    SpriteOut in [[stage_in]],
    texture2d<float> tex [[texture(0)]],
    sampler tex_sampler [[sampler(0)]])
{
    float4 texel = tex.sample(tex_sampler, in.uv);
    float fade_x = edge_factor(in.quad_uv.x, in.edge_fade.x, in.edge_fade.y);
    float fade_y = edge_factor(in.quad_uv.y, in.edge_fade.z, in.edge_fade.w);
    float4 color = texel * in.tint;
    if (in.texture_mask > 0.5) {
        color = float4(in.tint.rgb, texel.a * in.tint.a);
    }
    color.a *= min(fade_x, fade_y);
    return color;
}

fragment float4 sprite_yuv_fragment(
    SpriteOut in [[stage_in]],
    texture2d<float> y_tex [[texture(0)]],
    texture2d<float> u_tex [[texture(1)]],
    texture2d<float> v_tex [[texture(2)]],
    sampler tex_sampler [[sampler(0)]],
    constant float4 &levels [[buffer(0)]],
    constant float4 &coeffs [[buffer(1)]])
{
    float y = y_tex.sample(tex_sampler, in.uv).r * levels.x + levels.y;
    float u = u_tex.sample(tex_sampler, in.uv).r * levels.z + levels.w;
    float v = v_tex.sample(tex_sampler, in.uv).r * levels.z + levels.w;
    float4 texel = float4(
        y + coeffs.x * v,
        y + coeffs.y * u + coeffs.z * v,
        y + coeffs.w * u,
        1.0);
    float fade_x = edge_factor(in.quad_uv.x, in.edge_fade.x, in.edge_fade.y);
    float fade_y = edge_factor(in.quad_uv.y, in.edge_fade.z, in.edge_fade.w);
    float4 color = texel * in.tint;
    if (in.texture_mask > 0.5) {
        color = float4(in.tint.rgb, in.tint.a);
    }
    color.a *= min(fade_x, fade_y);
    return color;
}

struct MeshVertex {
    packed_float2 pos;
    packed_float4 color;
};

struct MeshOut {
    float4 pos [[position]];
    float4 color;
};

vertex MeshOut mesh_vertex(
    uint vertex_id [[vertex_id]],
    const device MeshVertex *vertices [[buffer(0)]],
    constant float4x4 &proj [[buffer(1)]])
{
    MeshVertex input = vertices[vertex_id];
    MeshOut out;
    out.pos = proj * float4(float2(input.pos), 0.0, 1.0);
    // Engine cameras use [-w, w] depth; Metal clips to [0, w].
    out.pos.z = (out.pos.z + out.pos.w) * 0.5;
    out.color = float4(input.color);
    return out;
}

fragment float4 mesh_fragment(MeshOut in [[stage_in]])
{
    return in.color;
}

struct TexturedMeshVertex {
    packed_float4 normal;
    packed_float3 pos;
    packed_float2 uv;
    packed_float4 color;
    packed_float2 tex_matrix_scale;
};

struct TexturedMeshInstance {
    packed_float4 model_col0;
    packed_float4 model_col1;
    packed_float4 model_col2;
    packed_float4 model_col3;
    packed_float4 tint;
    packed_float2 uv_scale;
    packed_float2 uv_offset;
    packed_float2 uv_tex_shift;
    float texture_mask;
    float cull_back;
    packed_float4 sphere_rows[3];
    packed_float4 additive_uv;
    ulong additive_texture;
};

struct TexturedMeshOut {
    float2 additive_uv;
    float additive;
    float4 pos [[position]];
    float2 uv;
    float4 color;
    float texture_mask;
    float cull_back;
};

vertex TexturedMeshOut textured_mesh_vertex(
    uint vertex_id [[vertex_id]],
    uint instance_id [[instance_id]],
    const device TexturedMeshVertex *vertices [[buffer(0)]],
    const device TexturedMeshInstance *instances [[buffer(1)]],
    constant float4x4 &proj [[buffer(2)]])
{
    TexturedMeshVertex vertex_data = vertices[vertex_id];
    TexturedMeshInstance inst = instances[instance_id];
    float4x4 model = float4x4(
        float4(inst.model_col0), float4(inst.model_col1),
        float4(inst.model_col2), float4(inst.model_col3));

    TexturedMeshOut out;
    out.pos = proj * model * float4(float3(vertex_data.pos), 1.0);
    // Engine cameras use [-w, w] depth; Metal clips to [0, w].
    out.pos.z = (out.pos.z + out.pos.w) * 0.5;
    float2 primary = float2(vertex_data.uv);
    float2 secondary = primary;
    uint mode = uint(vertex_data.normal.w);
    if ((mode & 3u) != 0u && inst.texture_mask < 0.5) {
        float4 r0 = float4(inst.sphere_rows[0]);
        float4 r1 = float4(inst.sphere_rows[1]);
        float4 r2 = float4(inst.sphere_rows[2]);
        float3x3 a = transpose(float3x3(r0.xyz, r1.xyz, r2.xyz));
        float3x3 cof = float3x3(cross(a[1], a[2]), cross(a[2], a[0]), cross(a[0], a[1]));
        float3 n = cof * float3(vertex_data.normal.xyz) * sign(dot(a[0], cof[0]));
        float4 p = float4(float3(vertex_data.pos), 1.0);
        float3 eye = float3(dot(r0, p), dot(r1, p), dot(r2, p));
        float3 r = reflect(eye / max(length(eye), 1e-20f), n / max(length(n), 1e-20f));
        float2 uv = r.xy / max(2.0f * length(r + float3(0.0, 0.0, 1.0)), 1e-20f) + 0.5;
        if ((mode & 1u) != 0u) primary = uv;
        if ((mode & 2u) != 0u) secondary = uv;
    }
    out.additive = float((mode & 4u) != 0u);
    out.additive_uv = secondary * float2(inst.additive_uv.xy) + float2(inst.additive_uv.zw);
    out.uv = primary * float2(inst.uv_scale)
        + float2(inst.uv_offset)
        + float2(inst.uv_tex_shift) * (float2(vertex_data.tex_matrix_scale) - float2(1.0));
    out.color = float4(vertex_data.color) * float4(inst.tint);
    out.texture_mask = inst.texture_mask;
    out.cull_back = inst.cull_back;
    return out;
}

fragment float4 textured_mesh_fragment(
    TexturedMeshOut in [[stage_in]],
    bool front [[front_facing]],
    texture2d<float> tex [[texture(0)]],
    sampler tex_sampler [[sampler(0)]],
    texture2d<float> additive [[texture(1)]],
    sampler additive_sampler [[sampler(1)]])
{
    if (in.cull_back > 0.5 && !front) discard_fragment();
    float4 texel = tex.sample(tex_sampler, in.uv);
    float4 color = texel * in.color;
    if (in.texture_mask > 0.5) {
        color = float4(in.color.rgb, texel.a * in.color.a);
    } else if (in.additive > 0.5) {
        float4 reflection = additive.sample(additive_sampler, in.additive_uv);
        color = float4(min(color.rgb + reflection.rgb, float3(1.0)), color.a * reflection.a);
    }
    if (color.a <= (1.0 / 256.0)) {
        discard_fragment();
    }
    return color;
}

// Reset depth inside the current color pass without changing its color attachment.
vertex float4 depth_clear_vertex(uint i [[vertex_id]]) {
    const float2 p[3] = {float2(-1.0, -1.0), float2(3.0, -1.0), float2(-1.0, 3.0)};
    return float4(p[i], 1.0, 1.0);
}

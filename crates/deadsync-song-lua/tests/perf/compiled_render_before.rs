// Frozen render and rotation from 5ffd4a261; render receiver becomes an explicit scene parameter.
use super::*;

pub(super) fn rotate_for_test(v: &mut V3, angles: Option<[f64;2]>, inverse: bool) { rotate(v, angles, inverse); }
pub(super) fn render(scene: &mut CompiledSongScene) {
        let multiplayer = scene.joined.iter().all(|v| *v);
        for binding in &scene.bindings {
            let camera = scene
                .entities
                .iter()
                .find(|e| e.camera == Some(binding.player))
                .unwrap();
            let e = &scene.entities[binding.entity];
            let offset = if multiplayer {
                if binding.player == 0 {
                    -0.27 * scene.screen[1]
                } else {
                    0.25 * scene.screen[1]
                }
            } else {
                0.0
            };
            let low = if multiplayer && binding.player == 1 {
                0.0
            } else {
                -scene.screen[1]
            };
            let high = if multiplayer && binding.player == 0 {
                10.0
            } else {
                scene.screen[1]
            };
            let state = &mut scene.states[binding.actor];
            if let Some(mesh) = &e.mesh {
                for (dest, source) in scene.points.iter_mut().zip(&mesh.points) {
                    *dest = *source;
                    rotate(dest, e.rotation, false);
                    for a in 0..3 {
                        dest[a] += e.position[a] - camera.position[a];
                    }
                    rotate(dest, camera.rotation, true);
                    let z = dest[2];
                    if z <= 0.0 {
                        *dest = [f64::NAN; 3];
                    } else {
                        dest[0] = dest[0] * 1000.0 / z;
                        dest[1] = (-dest[1] * 1000.0 / z + offset).min(high).max(low);
                        dest[2] = -z;
                    }
                }
                let vertices = Arc::make_mut(scene.meshes[binding.actor].as_mut().unwrap());
                for (v, &(point, uv)) in vertices.iter_mut().zip(&mesh.corners) {
                    let p = scene.points[point];
                    v.pos = [p[0] as f32, p[1] as f32];
                    v.z = p[2] as f32;
                    v.uv = uv;
                }
                state.visible = true;
                if let Some(color) = e.color {
                    state.diffuse = color;
                }
                if let Some(uv) = e.translate {
                    state.texcoord_offset = Some([
                        (uv[0] * scene.frame as f64) as f32,
                        (uv[1] * scene.frame as f64) as f32,
                    ]);
                }
            } else if let Some(ball) = e.ball {
                let mut p = std::array::from_fn(|a| e.position[a] - camera.position[a]);
                rotate(&mut p, camera.rotation, true);
                let z = p[2];
                if z <= 0.0 {
                    state.visible = false;
                } else {
                    let x = p[0] * 1000.0 / z;
                    let y = -p[1] * 1000.0 / z + offset;
                    state.x = x as f32;
                    state.y = y as f32;
                    state.z = (-z) as f32;
                    // zoomto{width,height} divides by the loaded sprite's
                    // intrinsic dimensions. Ball artwork is not assumed 32px.
                    let size = ball.radius * 2.0 * 1000.0 / z;
                    let intrinsic = state.size.unwrap_or([32.0, 32.0]);
                    state.zoom_x = (size as f32) / intrinsic[0];
                    state.zoom_y = (size as f32) / intrinsic[1];
                    state.visible = low <= y && y <= high;
                }
            }
        }
    }

fn rotate(v: &mut V3, rotation: Option<[f64; 2]>, inverse: bool) {
    let Some([pitch, yaw]) = rotation else { return };
    let sp = pitch.sin();
    let cp = pitch.cos();
    let sy = yaw.sin();
    let cy = yaw.cos();
    let [x, y, z] = *v;
    *v = if inverse {
        [
            cy * x - sy * z,
            sp * sy * x + cp * y + sp * cy * z,
            cp * sy * x - sp * y + cp * cy * z,
        ]
    } else {
        [
            cy * x + sp * sy * y + cp * sy * z,
            cp * y - sp * z,
            -sy * x + sp * cy * y + cp * cy * z,
        ]
    };
}

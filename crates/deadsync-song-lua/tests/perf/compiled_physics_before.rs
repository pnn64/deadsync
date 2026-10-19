// Frozen from 20586889b; receivers become explicit scene parameters.
use super::*;

pub(super) fn initialize(scene: &mut CompiledSongScene) {
        scene.active = false;
        scene.last_music = 0.0;
        scene.update_left = 0.0;
        scene.timer_left = if scene.bruh_mode { 8.0 } else { 0.0 };
        scene.frame = 0;
        scene.remaining = if scene.join_mode {
            8
        } else {
            scene.initial_remaining
        };
        scene.running = true;
        scene.winner = None;
        scene.finish_left = None;
        scene.restart = None;
        scene.held = [[false; 4]; 2];
        scene.points.resize(
            scene.entities
                .iter()
                .filter_map(|e| e.mesh.as_ref().map(|m| m.points.len()))
                .max()
                .unwrap_or(0),
            [0.0; 3],
        );
    }

pub(super) fn tick(scene: &mut CompiledSongScene, output: &mut SongLuaLiveOutput) {
        // Preserve authored system and entity order; computations stay double
        // until values cross the native actor setter boundary.
        for e in &mut scene.entities {
            if let (Some(r), Some(s)) = (&mut e.rotation, e.spin) {
                r[0] += s[0];
                r[1] += s[1];
            }
            if !e.moves.is_empty() {
                e.move_progress += 1.0;
                if e.move_progress >= e.moves[e.move_index].1 {
                    e.move_progress = 0.0;
                    e.move_index = (e.move_index + 1) % e.moves.len();
                }
                e.physics.as_mut().unwrap().velocity = e.moves[e.move_index].0;
            }
        }
        if scene.frame + 1 >= 60 {
            for e in &mut scene.entities {
                let Some(player) = e.input else { continue };
                let h = scene.held[player];
                let dx = f64::from(h[1] as u8) - f64::from(h[0] as u8);
                let dz = f64::from(h[3] as u8) - f64::from(h[2] as u8);
                let magnitude = (dx * dx + dz * dz).sqrt();
                let (dx, dz) = if magnitude == 0.0 {
                    (dx, dz)
                } else {
                    (dx / magnitude, dz / magnitude)
                };
                let accel = if e.ball.unwrap().slippy { 0.003 } else { 0.01 };
                let v = &mut e.physics.as_mut().unwrap().velocity;
                for (axis, d) in [(0, dx), (2, dz)] {
                    let target = 0.3 * d;
                    if target == 0.0
                        || (target > 0.0) != (v[axis] > 0.0)
                        || target.abs() > v[axis].abs()
                    {
                        v[axis] = if v[axis] < target {
                            (v[axis] + accel).min(target)
                        } else {
                            (v[axis] - accel).max(target)
                        };
                    }
                }
            }
        }
        for e in &mut scene.entities {
            if let Some(p) = &mut e.physics {
                p.collision = false;
            }
        }
        for _ in 0..2 {
            for e in &mut scene.entities {
                if let Some(p) = &mut e.physics {
                    if p.gravity {
                        p.velocity[1] = (p.velocity[1] - 0.5 * 0.01).max(-0.25);
                    }
                    for a in 0..3 {
                        e.position[a] += 0.5 * p.velocity[a];
                    }
                }
            }
            for box_index in 0..scene.entities.len() {
                let entity = &scene.entities[box_index];
                let Some(half) = entity.half_box else {
                    continue;
                };
                let position = entity.position;
                let rotation = entity.rotation;
                let velocity = entity.physics.map_or([0.0; 3], |p| p.velocity);
                let slippy = entity.box_slippy;
                for ball in &mut scene.entities {
                    let Some(mut data) = ball.ball else { continue };
                    let mut closest = std::array::from_fn(|a| ball.position[a] - position[a]);
                    rotate(&mut closest, rotation, true);
                    for a in 0..3 {
                        closest[a] = closest[a].min(half[a]).max(-half[a]);
                    }
                    rotate(&mut closest, rotation, false);
                    let mut direction: V3 =
                        std::array::from_fn(|a| closest[a] + position[a] - ball.position[a]);
                    let distance = (direction[0] * direction[0]
                        + direction[1] * direction[1]
                        + direction[2] * direction[2])
                        .sqrt();
                    if distance > 0.0 && distance <= data.radius {
                        for a in 0..3 {
                            direction[a] /= distance;
                            ball.position[a] -= (data.radius - distance) * direction[a];
                        }
                        let physics = ball.physics.as_mut().unwrap();
                        let relative: V3 =
                            std::array::from_fn(|a| physics.velocity[a] - velocity[a]);
                        let mut t = relative[0] * direction[0]
                            + relative[1] * direction[1]
                            + relative[2] * direction[2];
                        if t > 0.0 {
                            physics.collision = true;
                            t = (1.0 + 0.5) * t + 0.1;
                            for a in 0..3 {
                                direction[a] = t * direction[a];
                                physics.velocity[a] = relative[a] + velocity[a] - direction[a];
                            }
                            if direction[1] < 0.0 {
                                data.bounce_height = ball.position[1];
                            }
                            data.slippy = slippy;
                            ball.ball = Some(data);
                        }
                    }
                }
            }
        }
        for e in &mut scene.entities {
            if let Some(ball) = &mut e.ball {
                if e.position[1] < -50.0 {
                    e.position = ball.spawn;
                    ball.bounce_height = e.position[1];
                    e.physics.as_mut().unwrap().velocity = [0.0; 3];
                }
            }
        }
        for i in 0..scene.entities.len() {
            if let Some((target, offset)) = scene.entities[i].follow {
                let target = &scene.entities[target];
                let position = target.position;
                let y = target.ball.unwrap().bounce_height.min(position[1]);
                for (a, value) in [position[0], y, position[2]].into_iter().enumerate() {
                    scene.entities[i].position[a] =
                        0.9 * scene.entities[i].position[a] + 0.1 * (value + offset[a]);
                }
            }
        }
        for i in 0..scene.entities.len() {
            let e = &scene.entities[i];
            let position = e.position;
            if let Some(half) = e.checkpoint {
                let mut checkpoint = 0;
                for ball in &mut scene.entities {
                    if let Some(b) = &mut ball.ball {
                        if inside(ball.position, position, half) && b.spawn != position {
                            b.spawn = position;
                            checkpoint += 1;
                        }
                    }
                }
                for _ in 0..checkpoint {
                    scene.sound(1, output);
                }
            }
        }
        for goal in &scene.entities {
            if let Some(half) = goal.goal {
                for ball in &scene.entities {
                    if ball.ball.is_some() && inside(ball.position, goal.position, half) {
                        scene.winner = ball.input;
                        scene.running = false;
                        output.compiled_music = Some(CompiledMusicAction::Stop);
                        scene.sound(2, output);
                    }
                }
            }
        }
        if scene
            .entities
            .iter()
            .any(|e| e.physics.is_some_and(|p| p.collision))
        {
            let sound = 3 + (3.0 * scene.random.real()).floor() as usize;
            scene.sound(sound.min(5), output);
        }
        scene.render();
        scene.frame += 1;
    }

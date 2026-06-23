use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_render::BlendMode;
use std::sync::Arc;

pub struct BuiltNotefield {
    pub layout_center_x: f32,
    pub field_actors: Vec<Arc<[Actor]>>,
    pub judgment_actors: Option<Vec<Arc<[Actor]>>>,
    pub combo_actors: Option<Vec<Arc<[Actor]>>>,
}

impl BuiltNotefield {
    pub fn empty(layout_center_x: f32) -> Self {
        Self {
            layout_center_x,
            field_actors: Vec::new(),
            judgment_actors: None,
            combo_actors: None,
        }
    }
}

pub fn actor_with_world_z(mut actor: Actor, world_z: f32) -> Actor {
    match &mut actor {
        Actor::Sprite { world_z: z, .. } | Actor::TexturedMesh { world_z: z, .. } => *z = world_z,
        _ => {}
    }
    actor
}

pub fn share_actor_range(actors: &mut Vec<Actor>, start: usize) -> Option<Vec<Arc<[Actor]>>> {
    if start >= actors.len() {
        return None;
    }
    let shared: Arc<[Actor]> = Arc::from(actors.drain(start..).collect::<Vec<_>>());
    actors.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children: Arc::clone(&shared),
        background: None,
        z: 0,
        tint: [1.0, 1.0, 1.0, 1.0],
        blend: Some(BlendMode::Alpha),
    });
    Some(vec![shared])
}

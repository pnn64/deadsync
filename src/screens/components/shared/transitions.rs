use crate::act;
use crate::engine::present::actors::Actor;
use crate::engine::space::{screen_height, screen_width};

pub fn fade_in_black_actor(duration: f32, z: i16) -> Actor {
    act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(z):
        linear(duration): alpha(0.0):
        linear(0.0): visible(false)
    )
}

pub fn fade_out_black_actor(duration: f32, z: i16) -> Actor {
    act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(z):
        linear(duration): alpha(1.0)
    )
}

pub fn fade_in_black(duration: f32, z: i16) -> (Vec<Actor>, f32) {
    (vec![fade_in_black_actor(duration, z)], duration)
}

pub fn fade_out_black(duration: f32, z: i16) -> (Vec<Actor>, f32) {
    (vec![fade_out_black_actor(duration, z)], duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(actor: &Actor) -> f32 {
        let Actor::Sprite { tint, .. } = actor else {
            panic!("expected solid quad sprite");
        };
        tint[3]
    }

    #[test]
    fn fade_in_black_starts_opaque() {
        let (actors, duration) = fade_in_black(0.4, 1100);
        assert_eq!(duration, 0.4);
        assert_eq!(actors.len(), 1);
        assert_eq!(alpha(&actors[0]), 1.0);
    }

    #[test]
    fn fade_out_black_starts_transparent() {
        let (actors, duration) = fade_out_black(0.3, 1200);
        assert_eq!(duration, 0.3);
        assert_eq!(actors.len(), 1);
        assert_eq!(alpha(&actors[0]), 0.0);
    }
}

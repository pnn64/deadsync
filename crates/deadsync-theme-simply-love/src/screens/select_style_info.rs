//! Simply Love's authored Double/Couple/Routine explanation bubbles.
use crate::{act, i18n::tr};
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_present::space::widescale;
use deadlib_render_core::{BlendMode, MeshVertex};
use std::{cell::RefCell, sync::Arc};

const HEIGHT: f32 = 132.0;
const TEXT_ZOOM: f32 = 1.04;

struct Geometry {
    width: f32,
    border: Arc<[MeshVertex]>,
    body: Arc<[MeshVertex]>,
}

#[derive(Default)]
pub(super) struct InfoBubbles {
    alpha: [f32; 5],
    geometry: RefCell<Option<Geometry>>,
}

impl InfoBubbles {
    pub fn update(&mut self, selected: usize, dt: f32) {
        for (index, alpha) in self.alpha.iter_mut().enumerate().skip(2) {
            *alpha = if index == selected {
                (*alpha + dt.max(0.0) / 0.12).min(1.0)
            } else {
                (*alpha - dt.max(0.0) / 0.08).max(0.0)
            };
        }
    }

    pub fn push(&self, actors: &mut Vec<Actor>, index: usize, pad: [f32; 2], fade: f32) {
        let key = match index {
            2 => "DoubleInfo",
            3 => "CoupleInfo",
            4 => "RoutineInfo",
            _ => return,
        };
        let alpha = self.alpha[index] * fade;
        if alpha <= 0.0 {
            return;
        }
        let width = widescale(390.0, 500.0);
        let mut geometry = self.geometry.borrow_mut();
        if geometry.as_ref().is_none_or(|value| value.width != width) {
            *geometry = Some(Geometry {
                width,
                border: capsule(width + 10.0, HEIGHT + 10.0),
                body: capsule(width, HEIGHT),
            });
        }
        let geometry = geometry.as_ref().expect("bubble geometry prepared");
        let offset_x = match index {
            2 => widescale(108.0, 150.0),
            4 => -widescale(108.0, 150.0),
            _ => 0.0,
        };
        let x = pad[0] + offset_x;
        let y = pad[1] - 206.0;
        actors.push(mesh(
            Arc::clone(&geometry.border),
            [x, y],
            [width + 10.0, HEIGHT + 10.0],
            [0.0, 0.0, 0.0, alpha],
            10,
        ));
        // The diamond tail points at the selected pair of pads.
        actors.push(
            act!(quad: align(0.5, 0.5): xy(pad[0], y + HEIGHT * 0.5 - 2.0):
            setsize(42.0, 42.0): rotationz(45.0): diffuse(0.0, 0.0, 0.0, alpha): z(10)),
        );
        actors.push(
            act!(quad: align(0.5, 0.5): xy(pad[0], y + HEIGHT * 0.5 - 5.0):
            setsize(30.0, 30.0): rotationz(45.0): diffuse(1.0, 1.0, 1.0, alpha): z(11)),
        );
        actors.push(mesh(
            Arc::clone(&geometry.body),
            [x, y],
            [width, HEIGHT],
            [1.0, 1.0, 1.0, alpha],
            12,
        ));
        let mut text = act!(text: align(0.5, 0.5): xy(x, y): zoom(TEXT_ZOOM):
            font("miso"): settext(tr("SelectStyle", key)):
            wrapwidthpixels((width - 44.0) / TEXT_ZOOM):
            maxwidth((width - 36.0) / TEXT_ZOOM):
            diffuse(0.0, 0.0, 0.0, alpha): z(13): horizalign(center));
        if let Actor::Text { max_height, .. } = &mut text {
            *max_height = Some((HEIGHT - 26.0) / TEXT_ZOOM);
        }
        actors.push(text);
    }
}

fn mesh(
    vertices: Arc<[MeshVertex]>,
    offset: [f32; 2],
    size: [f32; 2],
    tint: [f32; 4],
    z: i16,
) -> Actor {
    Actor::Mesh {
        align: [0.5, 0.5],
        offset,
        size: size.map(SizeSpec::Px),
        tint,
        vertices,
        visible: true,
        blend: BlendMode::Alpha,
        z,
    }
}

fn capsule(width: f32, height: f32) -> Arc<[MeshVertex]> {
    let radius = (height * 0.5).min(width * 0.5);
    let mut perimeter = Vec::with_capacity(36);
    for (cx, cy, start) in [
        (width - radius, radius, -90.0_f32),
        (width - radius, height - radius, 0.0),
        (radius, height - radius, 90.0),
        (radius, radius, 180.0),
    ] {
        for segment in 0..=8 {
            let angle = (start + segment as f32 * 90.0 / 8.0).to_radians();
            perimeter.push([cx + angle.cos() * radius, cy + angle.sin() * radius]);
        }
    }
    let center = [width * 0.5, height * 0.5];
    let mut vertices = Vec::with_capacity(perimeter.len() * 3);
    for index in 0..perimeter.len() {
        for pos in [
            center,
            perimeter[index],
            perimeter[(index + 1) % perimeter.len()],
        ] {
            vertices.push(MeshVertex {
                pos,
                color: [1.0; 4],
            });
        }
    }
    vertices.into()
}

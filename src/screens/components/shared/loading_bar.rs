use crate::act;
use crate::engine::present::actors::{Actor, SizeSpec, TextContent};

const BORDER_PX: f32 = 2.0;

pub struct LoadingBarParams {
    pub align: [f32; 2],
    pub offset: [f32; 2],
    pub width: f32,
    pub height: f32,
    pub progress: f32,
    pub label: TextContent,
    pub fill_rgba: [f32; 4],
    pub bg_rgba: [f32; 4],
    pub border_rgba: [f32; 4],
    pub text_rgba: [f32; 4],
    pub text_zoom: f32,
    pub z: i16,
}

pub fn build(params: LoadingBarParams) -> Actor {
    let width = params.width.max(0.0);
    let height = params.height.max(0.0);
    let inner_w = (width - BORDER_PX * 2.0).max(0.0);
    let inner_h = (height - BORDER_PX * 2.0).max(0.0);
    let fill_w = inner_w * params.progress.clamp(0.0, 1.0);
    let text_max_w = (width - 12.0).max(0.0);

    let mut children = Vec::with_capacity(4);
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(width * 0.5, height * 0.5):
        zoomto(width, height):
        diffuse(
            params.border_rgba[0],
            params.border_rgba[1],
            params.border_rgba[2],
            params.border_rgba[3]
        ):
        z(0)
    ));
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(width * 0.5, height * 0.5):
        zoomto(inner_w, inner_h):
        diffuse(
            params.bg_rgba[0],
            params.bg_rgba[1],
            params.bg_rgba[2],
            params.bg_rgba[3]
        ):
        z(1)
    ));
    if fill_w > 0.0 {
        children.push(act!(quad:
            align(0.0, 0.5):
            xy(BORDER_PX, height * 0.5):
            zoomto(fill_w, inner_h):
            diffuse(
                params.fill_rgba[0],
                params.fill_rgba[1],
                params.fill_rgba[2],
                params.fill_rgba[3]
            ):
            z(2)
        ));
    }
    children.push(act!(text:
        font("miso"):
        settext(params.label):
        align(0.5, 0.5):
        xy(width * 0.5, height * 0.5):
        zoom(params.text_zoom):
        maxwidth(text_max_w):
        diffuse(
            params.text_rgba[0],
            params.text_rgba[1],
            params.text_rgba[2],
            params.text_rgba[3]
        ):
        z(3):
        horizalign(center)
    ));

    Actor::Frame {
        align: params.align,
        offset: params.offset,
        size: [SizeSpec::Px(width), SizeSpec::Px(height)],
        background: None,
        z: params.z,
        children,
    }
}

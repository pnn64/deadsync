//! Compare the production finalizer with its frozen pre-change implementation.
//! Private access follows masked_render: no benchmark hooks enter the library.
#[path = "../src/actors.rs"]
pub mod actors;
#[path = "../src/anim.rs"]
pub mod anim;
#[path = "../src/font.rs"]
pub mod font;
use deadlib_render_core::frame_compare;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;
#[path = "../src/space.rs"]
pub mod space;
#[path = "../src/texture.rs"]
pub mod texture;

pub mod compose {
    include!("../src/compose.rs");

    mod z_batching {
        include!("z_batching/cases.rs");
    }

    mod draw_order_recheck {
        include!("draw_order_recheck/cases.rs");
    }
}

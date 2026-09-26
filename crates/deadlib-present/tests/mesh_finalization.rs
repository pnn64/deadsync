//! Compare mesh finalization without adding benchmark hooks to the library.
#[path = "../src/actors.rs"]
pub mod actors;
#[path = "../src/anim.rs"]
pub mod anim;
#[path = "../src/font.rs"]
pub mod font;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;
#[path = "../src/space.rs"]
pub mod space;
#[path = "../src/texture.rs"]
pub mod texture;

pub mod compose {
    include!("../src/compose.rs");

    mod mesh_finalization {
        include!("mesh_finalization/cases.rs");
    }
}

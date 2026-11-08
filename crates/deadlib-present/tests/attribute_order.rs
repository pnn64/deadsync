//! Compare attribute event ordering using the production composition code.
#[path = "../src/actors.rs"]
pub mod actors;
#[path = "../src/anim.rs"]
pub mod anim;
#[path = "../src/font.rs"]
pub mod font;
#[path = "../src/space.rs"]
pub mod space;
#[path = "../src/texture.rs"]
pub mod texture;

pub mod compose {
    include!("../src/compose.rs");

    mod attribute_order {
        include!("attribute_order/cases.rs");
    }
}

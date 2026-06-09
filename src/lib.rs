// Lets in-crate code refer to this crate as `deadsync::…` (same paths the hot
// cdylib uses when it `#[path]`-includes screen render modules).
extern crate self as deadsync;

pub mod app;
pub mod assets;
pub mod config;
pub mod engine;
pub mod game;
// Hot-reload boundary ABI types: header/vtable/hash handshake shared by the host
// and the reloadable `deadsync-screens` cdylib. Definitions only. Gated behind
// the dev-only `hot` feature so release/normal builds carry neither this module
// nor the `deadsync-hot` runtime dependency edge.
#[cfg(feature = "hot")]
pub mod hot;
pub mod screens;
pub mod test_support;

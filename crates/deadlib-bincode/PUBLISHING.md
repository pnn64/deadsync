# Publishing

Rust requires the runtime library and its procedural macros to remain separate
packages. Publish the two packages in dependency order:

1. `deadlib-bincode-derive`
2. `deadlib-bincode`

Before a release:

```text
cargo test -p deadlib-bincode-derive
cargo test -p deadlib-bincode --all-features
cargo check -p deadlib-bincode --no-default-features
cargo check -p deadlib-bincode --no-default-features --features alloc
cargo check -p deadlib-bincode --no-default-features --features derive
cargo check -p deadlib-bincode --all-targets --all-features
```

Run `cargo publish --dry-run` and then `cargo publish` for each package before
moving to its dependent package. Confirm that `Cargo.lock` contains no packages
named `bincode`, `bincode_derive`, `unty`, or `virtue` before tagging the release.

Main and derive versions remain aligned. The derive crate is an implementation
detail nested under `derive/`; consumers should enable the main crate's default
`derive` feature rather than depending on it directly.

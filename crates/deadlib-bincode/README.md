# deadlib-bincode

`deadlib-bincode` is DeadSync's maintained, project-scoped fork of bincode 2.0.1.
It preserves the standard bincode 2.0.1 wire format while keeping only the API
and type implementations used by DeadSync persistence.

This is not an official continuation of the original bincode project. The exact
source provenance and the small adaptations made for this fork are recorded in
[`PROVENANCE.md`](PROVENANCE.md). Original copyright and license notices are
preserved in this package.

The derive implementation lives under `derive/` in this project because Rust
requires procedural macros to compile as a separate crate. Virtue's required
code-generation code is embedded there rather than maintained as another
package.

## Supported surface

- `encode_to_vec`, reusable `encode_into_vec`, owned `decode_from_slice`,
  allocation-free `borrow_decode_from_slice`, and `config::standard()`
- Compile-time decode limits through `Configuration::with_limit`
- `Encode`, `Decode`, and `BorrowDecode` traits and derives
- Booleans, integer and floating-point primitives, strings, vectors, options,
  arrays, tuples up to three elements, boxed slices, hash maps, and hash sets
- Manual implementations through `enc::Encoder`, `de::Decoder`, and
  `de::BorrowDecoder`

Serde integration, `no_std`/feature combinations, stream I/O helpers, atomics,
and unused standard-library container/type implementations are intentionally not
part of this fork.

## Compatibility policy

- `config::standard()` must remain byte-for-byte compatible with bincode 2.0.1.
- DeadSync's supported surface is retained unless a major release documents a
  migration.
- Wire-format changes require a major version and explicit migration support.
- Security, soundness, supported-Rust, and measured performance fixes may ship in
  patch releases when they preserve API and wire compatibility.
- The minimum supported Rust version is 1.86.0. Raising it is a breaking change.

Golden bincode 2.0.1 fixtures and focused persistence tests enforce this policy.

## Use

The package is intentionally aliased as `bincode` so existing source and derive
output continue to resolve `::bincode`:

```toml
[dependencies]
bincode = { package = "deadlib-bincode", version = "=2.0.1" }
```

```rust
use bincode::{Decode, Encode};

#[derive(Debug, PartialEq, Encode, Decode)]
struct Point {
    x: i32,
    y: i32,
}
```

## Untrusted input

The standard configuration has no byte limit. Decode files or messages that can
be influenced by another party with a schema-appropriate
`config::standard().with_limit::<N>()` configuration.

See [`SECURITY.md`](SECURITY.md) for the maintenance and reporting policy.

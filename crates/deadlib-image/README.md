# deadlib-image

Independent pure-Rust image libraries. `deadlib-image` contains precision-preserving
sample storage; each format is a separate Cargo package under its own directory.
`png/` is `deadlib-image-png`. Future `jpeg/`, `gif/`, and `webp/` packages will follow
the same layout. No application integration is enabled.

The PNG implementation targets the [W3C PNG Third Edition](https://www.w3.org/TR/png-3/),
[RFC 1950](https://www.rfc-editor.org/rfc/rfc1950), and
[RFC 1951](https://www.rfc-editor.org/rfc/rfc1951). Production dependencies are only
the local shared primitive crate. Reference codecs are development dependencies.

See [PNG coverage and work plan](png/COVERAGE.md) for the verified state and remaining
work. Registration in the workspace does not replace any existing image loader.

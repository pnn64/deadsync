# deadlib-image

Independent pure-Rust image libraries. `deadlib-image` contains precision-preserving
sample storage; each format is a separate Cargo package under its own directory.

| Directory | Package | Runtime dependencies |
| --- | --- | --- |
| `png/` | `deadlib-image-png` | Local shared primitives |
| `gif/` | `deadlib-image-gif` | Local shared primitives |
| `jpeg/` | `deadlib-image-jpeg` | Local shared primitives |
| `bmp/` | `deadlib-image-bmp` | None; owns its indexed/RGB/RGBA model |

These four formats cover the normal ITGmania image-loading scope. Native codec
integration into the application remains a separate migration step.

The PNG implementation targets the [W3C PNG Third Edition](https://www.w3.org/TR/png-3/),
[RFC 1950](https://www.rfc-editor.org/rfc/rfc1950), and
[RFC 1951](https://www.rfc-editor.org/rfc/rfc1951). See each format's README for its
coverage and compatibility policies. Reference codecs used by PNG/GIF tests are
development dependencies; the BMP crate has no runtime or development dependencies.

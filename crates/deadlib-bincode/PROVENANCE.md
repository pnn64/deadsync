# Source provenance

The fork starts from the exact source archives published to crates.io. Archive
hashes are SHA-256:

| Component | Version | crates.io archive SHA-256 | Published VCS commit |
|---|---:|---|---|
| `bincode` | 2.0.1 | `36eaf5d7b090263e8150820482d5d93cd964a81e4019913c972f4edcc6edb740` | `4673360aa638b2b907dff24538de54f258d157da` |
| `bincode_derive` | 2.0.1 | `bf95709a440f45e986983918d0e8a1f30a9b1df04918fc828670606804ac3c09` | `4673360aa638b2b907dff24538de54f258d157da` |
| `unty` | 0.0.4 | `6d49784317cd0d1ee7ec5c716dd598ec5b4483ea832a2dced265471cc0f690ae` | `5455639ae05f42202a270f16bb4d79f6791d9b70` |
| `virtue` | 0.0.18 | `051eb1abcf10076295e815102942cc58f9d5e3b4560e46e53c21e8ff6f3af7b1` | `1ecc01325a038c4f28aa8de25caae7c6eb91c66f` |

The original bincode README is retained as `UPSTREAM-README.md`. The nested
derive crate retains the bincode-derive and Virtue upstream READMEs separately.
Original license files are kept beside each incorporated component.

## Initial fork adaptations

- Rename the publishable packages to `deadlib-bincode` and
  `deadlib-bincode-derive` while retaining the Rust crate names `bincode` and
  `bincode_derive`.
- Internalize the single `unty` operation used by `deadlib-bincode` and embed
  the required Virtue implementation privately in the nested derive crate so
  neither abandoned package remains in the dependency graph.
- Remove the bincode 1 comparison from the string benchmark so the maintenance
  advisory is not reintroduced as a development dependency.
- Add safety comments required by DeadSync's unsafe-code policy.
- Add immutable bincode 2.0.1 wire fixtures.
- Set the fork's initial MSRV to Rust 1.86 so its maintained benchmark stack can
  use current Criterion releases.
- Reduce the runtime crate to DeadSync's persistence surface: owned in-memory
  encode/decode, derives, standard containers, standard configuration, and
  compile-time decode limits.
- Remove Serde, `no_std` feature combinations, atomic and stream-I/O support,
  unused standard-library types, tuple arities above three, and their tests.
- Replace the broad upstream integration suite with focused retained-surface,
  malformed-input, decode-limit, and immutable wire-compatibility tests while
  keeping benchmarks for string graphs, varints, and limited decoding.

These adaptations intentionally reduce the public API. They do not change bytes
produced by the retained surface under `config::standard()`.

## Performance adaptations

- Add `encode_into_vec` so repeated persistence writes can retain capacity and
  avoid allocator traffic after warmup.
- Restore slice-backed borrowed decoding for allocation-free strings and byte
  slices.
- Batch native-endian floating-point slices and fixed-width integer slices into
  contiguous copies while retaining scalar fallbacks for other configurations
  and reader types.
- Reuse caller-owned vector capacity during top-level vector decoding.
- Decode variable-width integer vectors through type-specialized loops that
  avoid repeated generic decoder bookkeeping.
- Emit each multi-byte variable-width integer with one writer call containing
  both its marker and payload.
- Reuse caller-owned string, hash-map, and hash-set allocations during
  top-level decoding.
- Reuse caller-owned vector, hash-map, and hash-set storage while decoding
  elements that borrow directly from the input slice.
- Add allocation-free encoded-size queries and encoding into caller-provided
  byte slices.
- Decode native-endian floating-point and fixed-width integer arrays with one
  contiguous copy while preserving scalar reader and endian fallbacks.
- Reuse allocations owned by strings and nested byte, numeric, and string
  vectors during caller-owned vector decoding.
- Pool and reuse allocations owned by string hash-map keys, string hash-map
  values, and string hash-set elements during caller-owned decoding.
- Pool and reuse allocations owned by byte-buffer hash-map keys, byte-buffer
  hash-map values, and byte-buffer hash-set elements, including mixed maps.
- Add paired old/new Criterion benchmarks with throughput, allocation churn,
  and Windows thread-cycle reporting. Run them with
  `cargo bench -p deadlib-bincode --bench performance -- --noplot` and
  `cargo bench -p deadlib-bincode --bench additional_performance -- --noplot`,
  `cargo bench -p deadlib-bincode --bench second_pass_performance -- --noplot`,
  `cargo bench -p deadlib-bincode --bench third_pass_performance -- --noplot`,
  `cargo bench -p deadlib-bincode --bench fourth_pass_performance -- --noplot`,
  `cargo bench -p deadlib-bincode --bench fifth_pass_performance -- --noplot`,
  `cargo bench -p deadlib-bincode --bench sixth_pass_performance -- --noplot`,
  and `cargo bench -p deadlib-bincode --bench seventh_pass_performance -- --noplot`.

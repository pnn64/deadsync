# Fuzzing

`decode` runs the bounded pixel decoder (`decode_with_upsampling`, with linear
or nearest upsampling chosen by input length) and the portable reference
pipeline (coefficient, lossless or hierarchical decoding, then planes, then
pixels) on the same input under the same limits. They must accept and reject
the same inputs and produce identical pixels; the only tolerated difference
is the reference exceeding a storage limit that the bounded row window stays
within. Panics, overflow checks (cargo-fuzz enables debug assertions) and
AddressSanitizer reports are failures.

On x86-64 CPUs with AVX2 the bounded decoder takes its AVX2-dispatched
kernels, so the oracle also compares them with the portable transform and
color conversion. Building with `--cfg deadlib_image_jpeg_portable` fuzzes
the baseline kernels instead.

With cargo-fuzz and nightly Rust, run from this directory:

```sh
cargo fuzz run decode -- -max_total_time=60 -max_len=262144
```

For an audited ASan campaign with SanitizerCoverage feedback restricted to the
native image crates, run from the repository root on Linux:

```sh
python3 crates/deadlib-image/jpeg/scripts/fuzz_seeds.py /tmp/jpeg-seeds.zip
python3 crates/deadlib-image/jpeg/scripts/fuzz_native.py \
  --cargo-fuzz /path/to/cargo-fuzz --seed-archive /tmp/jpeg-seeds.zip \
  --target-dir /tmp/jpeg-fuzz-build --output-dir /tmp/jpeg-fuzz-evidence \
  --runs 200000 [--portable]
```

Use fresh output and target directories. The seed archive holds every fixture
JPEG and repository asset JPEG of at most 256 KiB, built deterministically.
The runner records commands, source/binary/seed hashes, execution counts and
compiler invocations, and audits compiled object sections to confirm that the
harness and its dependencies provide no SanitizerCoverage feedback while
retaining ASan checks. Native-crate standard-library monomorphizations can
contribute feedback, so this is not a source-only coverage report or proof of
conformance.

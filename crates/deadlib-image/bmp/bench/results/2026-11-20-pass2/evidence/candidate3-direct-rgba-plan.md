# Candidate 3: append pixels already in RGBA byte order

Status: source-only proposal. Production remains frozen at `candidate2-rgba4444`.
The patch has passed `git apply --check`; it has not been applied, compiled,
tested, or timed. `candidate3-direct-rgba.json` records the exact source hashes
and patch SHA-256.

## Scope and reason

A general removal of truecolor initialization would require replacing the fixed
output-slice converters with initialized Vec builders. Appending each flattened
pixel to Vec<u8> can introduce length/capacity checks in the pixel loop; preserving
the exact-size iteration benefits would instead need typed Vec<[u8; 3/4]>
builders and wider changes to masked dispatch. Those changes are unnecessary for
the identity-copy case, where the existing work is exactly zero-fill then copy.

The proposed path recognizes explicit 32-bit masks
`[0xff, 0xff00, 0xff0000, 0xff000000]`. After the same fallible exact reservation,
top-down pixels use one `extend_from_slice(source)`; bottom-up pixels append
whole source rows in reverse order. The obsolete identity-copy arm is removed
from `masked_row`. Production change: 23 added lines, 3 removed lines. No unsafe
code, extra heap storage, generic buffer abstraction, or public API is added.

This is an existing measured workload, named `rgba32-swapped` by the harness.
Baseline decode cycles are 161,024.25 / 155,788.60 for 511×257 bottom/top and
7,660,623.17 / 7,386,686.83 for 1920×1080 bottom/top. These values identify the
target, not the size of any unmeasured gain. All eight default-size cases have
exact native/image output agreement. The baseline 1920×1080 bottom case has a
large maximum-batch outlier; use new paired measurements, not that maximum.

## Behavior and allocation argument

1. Header validation, configured limits, mask validation, and the complete source
   extent check all remain before allocation. The new branch calls the same
   `reserve(&mut samples, header.decoded_bytes)` once on Vec<u8>.
2. The gate requires 32-bit pixels, the exact four byte masks, explicit alpha,
   and `opaque_if_zero: false`. Implicit BGRA alpha detection and all canonical
   BGRA/opaque conversion paths remain unchanged. Explicit all-zero alpha stays
   transparent.
3. For 32-bit input, row_pitch = width × 4 and source_bytes = decoded_bytes;
   there is no row padding. The source slice already excludes header gaps and
   trailing bytes. Each append fits the capacity reserved before any append,
   so no further allocation can be requested.
4. Safe Vec extension initializes every returned byte. Top-down input already
   has logical row order; reversing the row iterator for bottom-up input leaves
   the byte order inside each pixel and row unchanged.
5. The removed identity arm cannot be needed by another current caller:
   masked_row is private to this decode loop, and parse_header sets implicit
   alpha only for canonical BGRA masks. All nonidentity formats use their
   original converters.

## Required validation after an authorized apply/build

The patch adds an independent wire test across DIB 56/108/124, widths 1/3/17/257,
both row orientations, all-zero and varying explicit alpha, nonzero source gaps
and trailing bytes, exact decoded-byte limits, one-byte truncation, and a limit
one byte too small. Existing mask-permutation tests continue to cover generic
fallbacks.

Use the existing standalone validation workspace and full 22,570-record native
snapshot. Freeze new timing/allocation/snapshot executables and verify the same
622 allocation rows. Requested allocation calls/bytes should be exactly
unchanged; this candidate reduces initialization traffic, not allocated storage.

## Timing gate

Compare against the immediate retained parent; rebuild first if either pending
parent is rejected. Do not reuse a descendant of rejected code.

Use the unchanged harness at CPU 2, five batches, separate timing and allocation
binaries, with reversed parent/child run order between decode and consuming RGBA.
The initial focus is `--filter 32` across all four existing sizes: this includes
the identity target plus canonical BGRA, implicit alpha, all-zero alpha, explicit
standard RGBA, and BGRX controls. Add the existing `--filter rgb333` generic-mask
cases to detect dispatch/code-layout regressions outside the target. Every
native/image policy difference remains logged, and every per-input loss must be
reported. A 40 ms focus should take roughly 55 seconds across both modes and both
executables, excluding setup; actual duration depends on the existing output
modes. Repeat any retained effect at 80 ms before a full matrix decision.


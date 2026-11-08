# Paired-spectrum inverse MDCT

The inverse transform follows the same unnormalised cosine equation and signed
MDCT symmetry as the original implementation. Its DCT-IV factorization uses a
complex FFT of length N/4 instead of zero-padding to length N.

Let the Vorbis block size be N, M = N/2, L = N/4, and x the M real spectral
coefficients. Define the length-M DCT-IV:

```text
D[r] = sum(j=0..M-1) x[j] cos(pi/M (j+1/2)(r+1/2)).
```

Pair even coefficients with reversed odd coefficients. For j = 0..L-1:

```text
a[j] = x[2j]
b[j] = x[M-1-2j]
z[j] = (a[j] + i b[j]) exp(-i pi j/M)
Z[k] = FFT_L(z)[k]                  (negative-exponent, unscaled FFT)
W[k] = Z[k] exp(-i pi (k+1/4)/M)

D[2k]       =  Re W[k]
D[M-1-2k]   = -Im W[k].
```

For the even-frequency result, the paired cosine phases are theta and
`pi(2k+1/2)-theta`; their cosines are cos(theta) and sin(theta). Thus their
contribution is the real part of `(a+i b) exp(-i theta)`. Expanding theta gives
`2pi jk/L + pi j/M + pi(k+1/4)/M`, exactly the FFT and two rotations above.
For the reversed odd-frequency result, the coefficients become sin(theta) and
`-cos(theta)`, giving the negative imaginary part. M is even for every legal
Vorbis block size.

The existing signed permutation places `-D[r]` at output index
`3M/2-1-r`; first/last quarters follow by exact MDCT symmetry. The implementation
writes the paired real/imaginary results directly into that middle region.
It fills every FFT input slot, eliminating the old zero fill. Phase tables are
computed and retained in f64. FFT scratch and arithmetic also use f64, with
rounding to f32 at the transform output. An eight-stage synthetic residue
followed by a nearly silent overlap exceeded the unchanged per-block PCM bound
with the former f32 FFT. See [configuration parity](CONFIGURATION_PARITY.md).

Owned transform elements occupy 14N bytes: L four-double rotation entries,
L/2 two-double twiddles, and L two-double scratch entries. The former f32
implementation used 7N bytes. Decoder and encoder memory preflight/reporting
include the wider elements; window slopes remain f32. Repeated transforms
still allocate nothing. Historical performance/resource reports describe their
recorded source revisions and have not been rerun for this precision change.

The independent direct cosine tests cover every legal block size, selected
output positions and repeated zero input. libvorbis transform/raw PCM/full Ogg,
upstream coded-grid, historical floor-zero, reset/allocation and seek gates must
pass without increasing their numerical tolerances. Performance measurements
use the resident transform worker rather than inferring kernel time solely from
whole-file throughput; see [PERFORMANCE.md](PERFORMANCE.md).

## Native forward analysis

The inverse has the form `P D x`, where D is the symmetric, unscaled DCT-IV and
P is the signed MDCT expansion/permutation. Forward analysis therefore computes
`(4/N) D P^T t`, using the same N/4 complex FFT and rotations. For M=N/2,
the transposed fold is:

```text
f[r] = -t[3M/2-1-r] - t[3M/2+r]     for 0 <= r < M/2
f[M/2+q] = t[q] - t[M-1-q]          for 0 <= q < M/2
X = (4/N) DCT-IV(f).
```

`Mdct` uses this fold directly while filling the bit-reversed FFT scratch; no
extra folded vector or allocation is needed. Its table/scratch storage is
14N bytes. Both transforms share a private FFT butterfly helper; analysis folds,
normalization and rotations use f64 before the final f32 coefficient store.

Independent tests compare analysis to the f64 direct cosine equation at every
legal size, then verify multiblock analysis/window/synthesis overlap before
quantization. A separate libvorbis `mdct_forward` oracle compares every
coefficient at all eight sizes, with peak <=3e-7 and RMS <=5e-8 at unit peak.
Observed maximum coefficient error is 8.196e-8. Encoder output is independently
decoded through libvorbisfile and Symphonia; see [ENCODING.md](ENCODING.md).

## Unit FFT twiddles

The radix-two FFT's first stage uses only the twiddle `exp(0) = 1`.
It now processes adjacent complex pairs in a contiguous sum/difference pass.
Each later group handles its first pair the same way before evaluating its
remaining complex twiddles. For an FFT of length L = N/4, this removes L-1
complex multiplications, out of `(L/2) log2(L)` butterflies. At N=2048 that is
511 of 2,304 butterflies. The same helper serves forward and inverse transforms.

Tables, scratch storage, bit reversal, rotations and the remaining butterflies
are unchanged. No transform coefficient is approximated. Omitting multiplication
by signed zero can change the sign of a zero result; finite PCM is checked with
the existing numerical tolerances. Arbitrary nonfinite transform input has no
new numerical guarantee. Packet synthesis retains its finite-value checks.

Paired kernel and complete decoding results, including unchanged accuracy gates,
are recorded in [PERFORMANCE.md](PERFORMANCE.md).

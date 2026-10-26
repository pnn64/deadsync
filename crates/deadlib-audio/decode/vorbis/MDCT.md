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
computed in f64 once and stored as f32; synthesis arithmetic remains f32.

Owned transform elements shrink from 20N to 7N bytes: L four-float rotation
entries, L/2 two-float twiddles, and L two-float scratch entries. Decoder memory
accounting includes the same window slopes and updates accordingly. No new
allocation, unsafe code, SIMD, dependency or approximation is introduced.

The independent direct cosine tests cover every legal block size, selected
output positions and repeated zero input. libvorbis transform/raw PCM/full Ogg,
upstream coded-grid, historical floor-zero, reset/allocation and seek gates must
pass without increasing their numerical tolerances. Performance measurements
use the resident transform worker rather than inferring kernel time solely from
whole-file throughput; see [PERFORMANCE.md](PERFORMANCE.md).

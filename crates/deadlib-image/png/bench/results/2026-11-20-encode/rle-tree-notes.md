# Analytic RLE tree candidate

This proposed patch changes only the optional Compression::Rle format choice.
Stored and Fast retain their defaults, token selection, fixed codes, stored
fallback comparison, allocation bounds, and encoded output. Production files
have not been changed by this worker.

## Independent analytic construction

Literal widths use the signed residual magnitude min(byte, 256-byte):

| Symbols | Width |
| --- | --- |
| Literal 0 | 2 |
| Magnitude 1, both signs | 4 |
| Magnitude 2, both signs | 5 |
| Magnitude 3, both signs | 6 |
| Magnitudes 4 through 7 | 7 |
| Magnitudes 8 through 15 | 8 |
| Magnitudes 16 through 38 | 9 |
| Magnitudes 39 through 128 | 10 |
| End of block 256 | 10 |
| Length symbols 257 through 284 | 8 |
| Length symbol 285 (258 bytes) | 5 |

Magnitude 128 has only one literal. The width counts are
2:1, 4:2, 5:3, 6:2, 7:8, 8:44, 9:46, 10:180.
Their weights in units of 1/1024 total
256+128+96+32+64+176+92+180 = 1024 exactly.
The canonical-code function checks completeness during constant evaluation.

This is an analytic prior, not a histogram from an input, corpus, or imported
implementation. The table is generated in symbol order at compile time.
Literal 0 is code 0 with 2 bits; length 285 is reversed code 14 with 5 bits;
EOB is reversed code 1023 with 10 bits. Distance symbol 0 has the sole
one-bit distance code, zero.

## Header derivation and cost

The code-length alphabet assigns two bits to symbols 10 and 16, and four
bits to symbols 1,2,4,5,6,7,8,9. Its Kraft sum is 2/4 + 8/16 = 1.
HLIT=29 describes 286 literal/length symbols; HDIST=0 describes one distance
symbol; HCLEN=14 describes 18 code-length entries in the specified DEFLATE
order. Every constant-width run emits its first width explicitly, then
code 16 for groups of 3 through 6 repeats, then up to two explicit widths.
Thus code 16 is never emitted without an established previous width.

The header builder runs entirely during Rust constant evaluation and uses
the same derived lengths as the encoding table. No tree is constructed at
runtime, and there is no header allocation. It produces 67 width tokens
and 332 bits after the three BFINAL/BTYPE bits: 335 bits, about 42 bytes,
per 32768-byte block. EOB adds 10 bits. The eleven stored u32 words have
352 bits of capacity, so unsupported header growth fails constant evaluation.

The current generated header-body words, in emission order, are:
9000b81d 200208a4 72b3d598 86eeef7e bbbbbbbb bbbbbbbb bbbbbbbb bbdbbbbb
cadf9dbb eeee7174 000001d2.
Ten whole words and twelve low bits of the last word are emitted.
These values document the derivation; the production patch computes them.

## Run and literal emission

The patch retains the separate four-literal scanning fast path. Its
four-zero guard remains conservative and correct with the new tree.
Each zero run is still seeded with an explicit literal, so matches never
require cross-block history. A length-258 match now takes six bits,
including the distance bit, instead of thirteen.

A nonmaximal zero match needs at least nine bits. Therefore a remaining
run of at most four zeros is emitted literally (at most eight bits).
The matching loop requires at least five remaining zeros. Its final
literal slice remains within the existing helper's four-byte limit.
Length, extra bits, and the final zero distance bit are emitted in one
Writer::write call. The combined token is at most fourteen bits.

Four literal codes need at most forty bits. With at most seven pending
bits, the grouped helper needs at most 47 bits of its u64 accumulator and
emits at most five completed bytes. The patch leaves its copy/append tail
unchanged, so it is compatible with the separately measured eight-byte
append followed by truncate.

## Capacity and finalization

Only Rle increases its buffer reservation:

- Streaming: 32768*10/8 + 64 = 41024 output bytes.
- Buffered: input length + ceil(input length/4) + 64.

The current header, EOB, pending bits, padding, wrapper, and temporary
eight-byte append fit within the 64-byte slack. For multiple blocks,
previous selected blocks cannot exceed their stored alternative; their
small per-block overhead is far below the extra quarter-byte per source
byte reserved for the tentative ten-bit block. Streaming scratch remains
below the existing 80 KiB test limit.

Fast emits its existing fixed EOB, then calls the factored original
stored-size comparison. Rle emits its own EOB before that comparison.
The comparison retains the same strict inequality and alignment formula.
Empty and short Rle blocks generally select Stored because of header cost;
this does not affect Fast or Stored output. Streaming progress, final
checksum emission, and separate finish remain in the existing code.

## Focused validation included in the proposed patch

Existing Rle run cases add lengths 5 and 6 at the new threshold.
Alternating leading literals 255 and 254 have widths 4 and 5, spanning
all eight bit offsets for leading counts 0 through 7.
The stored-fallback case uses bytes 39 through 217, which all require
ten bits, to exercise both the widest-code reservation and rollback.
The existing independent inflater, fragment, all-literal, PNG/APNG,
counter, scratch, and output-tail tests remain the validation gates.

No build, test, or timing has been run by this worker. In particular,
compiler acceptance and actual speed/size improvements are still for the
root's serialized validation. The fixed header cost and the analytic
prior's fit to real residuals are the principal measurement risks.

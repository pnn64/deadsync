# Bounded Huffman prefix lookup

Codeword assignment still follows the Vorbis entry-order tree. Decoding can
resolve up to eight leading LSB-first bits through a small table appended to
that tree's existing `Vec<[u32; 2]>`. It uses no additional vector and does not
change the scalar/VQ APIs. Growing the existing buffer can reallocate during
construction; packet decoding and reset allocate nothing.

The table has four bytes per prefix. Its width is bounded by eight, the longest
codeword, the number of used entries and the available setup budget. Singletons
and books whose maximum length is at most two keep the direct tree path. The
maximum table element cost is 1 KiB per book. Owned vector capacity, including
any spare capacity, remains included in setup storage reporting.

Each table word stores a 28-bit value and a four-bit tag. A nonzero tag is the
exact consumed length of a short leaf; the value is its entry number. A zero
tag points to the tree node reached after consuming the full prefix width.
Thus short words consume only their own bits, and long words resume the original
tree traversal. A successful speculative lookahead permits an internal checked
cache advance without a second refill/check. Public bitpacking behavior is
unchanged. If lookahead cannot obtain the full table width, decoding starts at
the tree root, preserving short words and exact end-of-packet behavior.

Construction traverses only the first eight tree levels. A short leaf fills all
suffix aliases: with low `d` prefix bits `p`, table indices are `p + k*2^d`.
Each slot corresponds to the same path that the scalar decoder would take.
Continuation slots store the node after that prefix, not its parent. Logical
node counts and `max_huffman_nodes` exclude the lookup slots.

Setup parsing builds optional tables **after** validating the entire mandatory
grammar. Tables use only the remaining setup budget, shrink or disappear when
it is insufficient, and fall back to the tree if optional allocation fails.
They cannot consume memory needed by later setup fields. Public `from_lengths`
inputs whose large sparse entry/node indices do not fit the packed table also
retain the tree path; the optimization adds no format restriction. Ordinary
Vorbis codebook entry indices already fit their 24-bit wire field.

Single-entry books still consume one bit and accept both values, following the
specification erratum. The tree grammar, invalid-tree errors and sticky
end-of-packet semantics remain strict. Tests compare accelerated/tree results,
consumed positions, remaining bits and exhausted state for every byte prefix,
all bit alignments, packet tails and a 32-bit-deep tree. Exact node-limit and
zero-spare-budget tests cover the fallback.

## Measurement workload

The isolated benchmark obtains codewords from pinned libvorbis encoding of all
active entries in real setup books. Both workers decode the same generated
length/codeword corpus and independently check every entry and final consumed
bit count before timing. Codebook parsing, table construction and corpus loading
are outside the timed scalar loop. The manifest's unit is **codewords**; the
shared TSV count/rate fields retain their historical `samples` spelling.

```powershell
python crates/deadlib-audio/decode/scripts/benchmark.py --huffman
```

Every active entry occurs once per iteration, in entry order. This weights rare
long words more than a real encoder typically would and is not an audio symbol
frequency trace. The original tree already outperformed libvorbis on these
workloads. A faster scalar loop alone therefore does not establish that it was
the full decoder's dominant bottleneck. Whole-file throughput and startup must
be measured separately; see [PERFORMANCE.md](PERFORMANCE.md).

The later [real-packet codebook diagnostic](CODEBOOK_PROFILE.md) records actual
per-book/per-entry counts and prefix/fallback activity, with every PCM sample's
bits checked against production. Its symbol distribution guides future
experiments; instrumented counters are not a timing benchmark.

The subsequent [bounded-prefix update](PREFIX_PATHS.md) inlines the prefix/VQ
wrappers and outlines the unchanged tree continuation and cache refill. Paired
song full-decode medians improve 1.047x/1.052x with unchanged allocation/IO
counters; startup and short-file movements are mixed. Rejected VQ/scatter
variants, exact PCM checks and sanitizer timeout/replay evidence are retained.

The runner can build native production sources from a baseline commit in a
fresh ignored workspace, retaining the current benchmark adapters and exact
dependency lock/profile settings. It interleaves the baseline, current native,
Symphonia and libvorbis workers in each trial, verifies decoded counts, and
records source/archive/binary hashes. The original checkout is unchanged.

```powershell
python crates/deadlib-audio/decode/scripts/benchmark.py --baseline-revision 6c5c948ed --repeats 7
```

This remains a shared-machine benchmark, not a CPU-isolated experiment. Retain
trial spreads, reference drift and startup costs alongside throughput results.

The recorded paired trials improve full decoding on the two longer songs,
while setup/table construction increases first-PCM time and makes the three
short fixtures slightly slower overall. These costs are included in the
performance report rather than presented as a universal speedup.

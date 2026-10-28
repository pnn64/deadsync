# MPEG-2.5 mixed-block geometry

MPEG-2.5 is a compatibility extension, not an ISO-standardized MPEG version.
The native 8-kHz mixed profile follows Sieler and Sperschneider's 1997
[Layer3 Bitstream Syntax and Decoding, issue 1.2](https://mp3guessenc.sourceforge.io/MPEG%20Layer3%20Bitstream%20Syntax%20and%20Decoding.pdf):
six long scalefactor bands span 72 lines/four polyphase subbands; short bands
start at band three. Alias reduction applies across the three boundaries within
the long region. The document explicitly specifies four subbands for alias
reduction at 8 kHz and normal windows for the mixed block's lower long region.

The earlier universal reference survey found substantial disagreement, which
was correctly left open rather than resolved by selecting whichever decoder
matched a proposed change. The new `tests/reference_mixed8.py` supplies an
independent direct-equation oracle for this specific geometry:

* Independently bit-write a mono mixed block with an isolated coefficient at
  each of all 576 frequencies; use book 1, gain 206 and alternating signs.
* Follow each active block with two zero blocks to expose and clear retained
  overlap and synthesis history. This produces 1,728 frames/995,328 samples.
* Calculate alias butterflies, long/short IMDCT and windows directly with NumPy
  matrices, then synthesize through a shifted 1,024-element history. No native
  Rust source, generated transform matrices or ring-buffer implementation is read.
* Use only the independently sourced band-boundary fixture and hash-pinned
  numeric synthesis-window table. Compare every native sample using the original
  `2e-6` RMS / `2e-5` maximum limits; no alignment/gain fit is performed.

Native output matches all 576 frequencies: RMS `3.175e-9`, maximum `2.980e-8`.
Against the same equations, libmad and mpg123 disagree at 44 isolated frequencies,
and Symphonia 0.6.1 at 552. The reference results remain recorded as disagreements,
not passing native-versus-reference comparisons. This resolves the question of
whether the native geometry matches the selected primary extension document;
it does not claim that every decoder implements that profile identically.

These constructed window sequences exercise mathematical reconstruction rather
than certify conforming encoder behavior. Normal LAME MPEG-2.5 files at all
three extension rates continue to pass all three reference PCM gates. No
reference-specific geometry workaround is enabled implicitly for real files.

Reproduce with NumPy and the existing decode/C oracle CLIs:

```sh
python tests/reference_mixed8.py --native /path/to/decode --oracle /path/to/decode-reference --dewindow /path/to/dewindow --output /new/output
```

From Windows, `--wsl-distro Debian` invokes the supplied Linux binaries through
WSL while retaining the local NumPy calculation. The output includes exact
commands, input/PCM/binary/script/numeric hashes and per-frequency disagreement
lists. The passing native equation gate and explicit reference disagreements are
archived in `tests/validation/2026-10-28-mixed8`.

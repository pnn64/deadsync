# Switched-block Huffman region correction

Mixed MPEG-2/2.5 blocks now count their implicit Huffman region in scalefactor
band entries. Region zero contains eight entries: six long bands followed by
short band three in windows zero and one. Consequently its exclusive encoded
line boundary is 48 at 11.025/12/16/22.05/24 kHz and 96 at 8 kHz. MPEG-1 mixed
blocks retain eight long bands/36 lines. Pure short blocks retain nine window
band entries/36 lines, or 72 lines at 8 kHz.

This follows the explicit LSF mixed-region explanation in section 4.2.4 of the
[1997 Layer3 syntax document](https://mp3guessenc.sourceforge.io/MPEG%20Layer3%20Bitstream%20Syntax%20and%20Decoding.pdf).
MPEG-2.5 remains a compatibility profile, including the separately established
72-line mixed long geometry at 8 kHz; see [MPEG25.md](MPEG25.md).

The previous implementation incorrectly applied the pure-short formula to all
short blocks. Earlier all-frequency tests used the same book on either side of
the boundary, which could not reveal this error. The new native regression
encodes positive (1,0) pairs with books one and two throughout all 576 lines,
then requires exact values and bit consumption at every rate in both shapes.
It failed on the original implementation at mixed 8 kHz line 72 and passes
after the correction.

`tests/reference_regions.py` independently writes 18 complete mono streams,
624 isolated signed-coefficient cases, both book orders, and lines surrounding
both the old and corrected boundaries. Spectra come directly from the known
encoded coefficient and mathematical reorder map. The matrix/shift-history
oracle computes complete stateful PCM, including two zero frames after each
case. All **1,368,576 native PCM values** pass the unchanged RMS `2e-6` and
maximum `2e-5` criteria; measured maximum is below `2.980e-8`.

The original decoder rejects all six mixed LSF streams. After correction, all
18 native streams pass. All nine pure-short streams also agree with libmad
0.15.1b, mpg123 1.32.10 and Symphonia 0.6.1. Mixed MPEG-1 agrees with libmad and
mpg123; mixed LSF except 8 kHz agrees with libmad. mpg123 and Symphonia differ
substantially on the remaining mixed cases, and libmad rejects the 8 kHz
mixed stream. These disagreements remain in the report. They do not alter the
independent expected spectra, selected profile, or numeric limits.

After correction, all 18 supplied legacy vector/PCM pairs, 84 LAME framing-grid
files, and 30 FFmpeg/LAME, Shine and Media Foundation files still pass their
existing PCM criteria. Symphonia's recorded free-format rejection remains an
explicit exclusion requiring both C references to pass.

Reports, hashes and package checks are in
[the validation directory](tests/validation/2026-10-28-regions/validation.json).
The package's 48 tests pass on Windows, Linux and the extracted offline manifest;
all-target Clippy and formatting pass. ASan runs execute 20,980 frame inputs,
622 enlarged-corpus stream replays, and 1,467 inputs in a subsequent stream
campaign that includes new mutations; all finish without failures or artifacts.
All 152 previously validated file outputs also remain bit-identical after this
correction. The initial stream run exhausted its time budget in seed loading;
the extended run is recorded separately and supplies mutation coverage.
External media and PCM stay in ignored scratch storage. Reproduce the equation
gate with NumPy and the built native/C public-API example wrappers:

```sh
python tests/reference_regions.py --native /path/to/decode --oracle /path/to/decode-reference --dewindow /path/to/dewindow --output /path/to/new-report
```

On Windows, `--wsl-distro Debian` runs the Linux wrappers while NumPy evaluates
the expected PCM locally. No decoder output supplies expected coefficients.

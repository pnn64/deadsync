# Independent stereo equation audit

The 90-stream survey in RAW_PCM.md deliberately exposed disagreement among
libmad, mpg123 and Symphonia. Matching any one reference was insufficient to
settle the affected intensity-stereo cases. `tests/reference_stereo_equations.py`
now checks the same input bytes against an independent mathematical oracle.

The bit writer supplies at most one known coefficient per encoded channel.
Expected spectra derive directly from those coefficient band/window labels,
transmitted pan positions, scalefactor gains and the MPEG intensity/MS equations.
The oracle does not search native output for zero bounds or reuse native stereo
processing. Short-window bounds are independent; a higher short coefficient
prevents intensity coding in the mixed long region. The uncoded top band uses
the preceding transmitted pan only when that preceding band is intensity coded;
otherwise it uses the balanced default. This distinction is explicitly clarified
in published ISO/IEC 14496-3:2001/Amd.3:2005, clause 9.5.5, alongside the earlier
MPEG-1/2 Layer III equations described by the 1997 Layer3 document.

An independent NumPy implementation then applies alias butterflies, direct
long/short inverse matrices and windows, overlap, inversion and a shifted
1,024-element synthesis history. It reads no native implementation or generated
transform matrices. Only the hash-pinned numeric band and synthesis-window
tables are shared factual inputs. Stateful output is compared sample-for-sample,
without alignment or gain fitting.

All 90 files pass the unchanged `2e-6` RMS / `2e-5` maximum gate: 40,628,736
sample values across nine sample rates, long/short/mixed blocks, all eight
three-bit pans, MS on/off, both LSF intensity scales, right-channel bounds and
uncoded top bands. The worst maximum error is below `2.981e-8`. This confirms the
selected equations across this constructed survey. The original reference
disagreements remain recorded; this result does not convert those comparisons
into passing universal parity or certify a conforming encoder corpus.

The separate legacy `l3-test45/46` supplied-PCM checks and ordinary encoded-file
comparisons remain independent end-to-end evidence. Exhaustive scalefactor tests
cover compression classes and widths beyond this survey's three-bit pans.

Run from the package directory with NumPy and the raw decode CLI:

```sh
python tests/reference_stereo_equations.py --native /path/to/decode --corpus /path/to/stereo-input --dewindow /path/to/dewindow --output /new/output
```

Windows can invoke a Linux CLI with `--wsl-distro Debian`. Reports pin source
bytes, PCM, binary, script and NumPy version. The archived results and source
manifest are in `tests/validation/2026-10-28-stereo-equations`.

Sources: [Layer3 syntax/decoding, issue 1.2](https://mp3guessenc.sourceforge.io/MPEG%20Layer3%20Bitstream%20Syntax%20and%20Decoding.pdf),
[published intensity clarification](https://cdn.standards.iteh.ai/samples/39584/696c43b397e24f5ea9b6f80f8b193464/ISO-IEC-14496-3-2001-Amd-3-2005.pdf).

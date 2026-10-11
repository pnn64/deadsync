The committed harness 0.1.46 (`17e23b4`) loaded these four PNG files through
ITGmania's pinned `RageSurface_Load_PNG.cpp` and libpng. `native.json` contains
the actual decoded pixels and bitmap upload metadata. Each `.rgba` is an exact
copy of the corresponding native `loaded.pixels` bytes. Source, executable,
codec and output hashes are in `provenance.json`; the local reference files
were also checked against the pinned source after line-ending normalization.

The inputs exercise RGB16, RGBA16, grayscale16 and grayscale-alpha16 samples.
Native PNG loading strips the low byte; generic image conversion rescales to
8-bit, which changes selected samples. The production decoder test compares
all 560 output bytes with zero tolerance, using default Sprite texture hints.

These files establish decoded pixel behavior. The 3x2 sources also show
native minimum-size stretching in upload metadata; that sizing behavior is
outside the raw decoding assertion. Sprite upload pixels are omitted because
unused native POT padding is not fully initialized. GPU conversion, physical
mipmaps and framebuffer parity remain unverified by this control.

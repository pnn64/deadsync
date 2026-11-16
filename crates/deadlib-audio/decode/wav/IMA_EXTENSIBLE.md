# Extensible IMA descriptors and validation

The IMA block codec now recognizes the WAVEFORMATEXTENSIBLE GUID corresponding
to tag 0x0011. The Samples union supplies the block's frame count; it is not a
PCM precision value. Three-bit and four-bit IMA retain their existing code widths,
word packing, arithmetic and geometry validation. Container parsing remains
independent of the codec and preserves the complete raw descriptor.

[Microsoft's WAVEFORMATEXTENSIBLE description](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible)
defines the compressed samples-per-block union, channel mask and tag/GUID mapping.
[WAVEFORMATEX](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex)
describes format-specific data following an extensible header. Applying those
general fields to IMA's existing block geometry is an interpretation; this does
not establish acceptance by every Windows audio API or certify every compressed
extensible profile. The nibble-width file mapping has direct 4-bit reader evidence
below. SDL also implements the IMA union mapping, while
[explicitly declining Microsoft ADPCM's unspecified coefficient placement](https://github.com/libsdl-org/SDL/blob/main/src/audio/SDL_wave.c).
An authoritative Microsoft coefficient layout was not located during this audit;
that remains unresolved, rather than defining a new private layout here.

`AdpcmSpec::format_extensible(endian, channel_mask, output)` emits a 40-byte IMA
descriptor into caller storage. It supports 3/4-bit geometry, both byte orders,
matching speaker masks, zero/unspecified masks and SPEAKER_ALL. Invalid arguments
leave the output unchanged. Use the descriptor with `Adpcm::encode_block` and
the codec-independent Muxer when a specific speaker mask is needed.

`AdpcmWriter` now honors `WriteOptions::force_extensible` for IMA, using an
unspecified mask. Both seekable and known-length output use this descriptor;
encoded block bytes and lossy quantizer decisions remain unchanged. The flag
previously had no effect in this writer. Requesting it for Microsoft ADPCM now
returns Unsupported before sink I/O. Ordinary Microsoft descriptors and their
coefficient tables retain the existing block/reader/writer paths. Format.valid_bits
is documented as the raw Samples word for an extensible block codec; native PCM
precision semantics remain unchanged.

Native Model texture requests and stretch metadata from harness 0.1.43.

The source is a 5x9 RGBA PNG. ModelTypes.cpp requests stretch, mipmaps and
hot-pink color-key processing for diffuse and secondary textures. The
RageBitmapTexture.cpp stretch branch sets image size to the power-of-two
allocation, 8x16. The old headless adapter ignored the explicit stretch
request and reported an image size of 5x9.

The repaired adapter reports 8x16 and retains the requested flags across
all three native diffuse/secondary/glow passes. `provenance.json` pins the
clean committed harness, executable, native source and exact input hashes.

This is request/metadata evidence. Pixel preprocessing, mip storage and
native framebuffer output remain unverified. DeadSync currently prepares
a 5x9 image without the Model's default mip/stretch/color-key processing;
that production gap remains open. Complete Model archives remain pending.

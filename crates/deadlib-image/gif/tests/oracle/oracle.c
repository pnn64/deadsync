/* Test-only giflib oracle. No C code is linked into the Rust codec. */
#include <gif_lib.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

static void number(uint32_t n) {
    unsigned char b[4] = {n, n >> 8, n >> 16, n >> 24};
    if (fwrite(b, 1, 4, stdout) != 4) exit(4);
}
static void palette(const ColorMapObject *p) {
    number(p ? p->ColorCount : 0);
    number(p ? p->SortFlag : 0);
    if (p) for (int i = 0; i < p->ColorCount; ++i) {
        unsigned char c[3] = {p->Colors[i].Red, p->Colors[i].Green, p->Colors[i].Blue};
        if (fwrite(c, 1, 3, stdout) != 3) exit(4);
    }
}
int main(int argc, char **argv) {
    if (argc != 2 && argc != 3) return 2;
#ifdef _WIN32
    _setmode(_fileno(stdout), _O_BINARY);
#endif
    int error = 0;
    GifFileType *g = DGifOpenFileName(argv[1], &error);
    if (!g) { fprintf(stderr, "open: %s\n", GifErrorString(error)); return 1; }
    if (DGifSlurp(g) == GIF_ERROR) { fprintf(stderr, "decode: %s\n", GifErrorString(g->Error)); DGifCloseFile(g, &error); return 1; }
    number(g->SWidth); number(g->SHeight); number(g->SColorResolution);
    number(g->SBackGroundColor); number(g->AspectByte); number(g->ImageCount);
    palette(g->SColorMap);
    for (int i = 0; i < g->ImageCount; ++i) {
        SavedImage *s = &g->SavedImages[i];
        GifImageDesc *d = &s->ImageDesc;
        number(d->Left); number(d->Top); number(d->Width); number(d->Height); number(d->Interlace);
        palette(d->ColorMap);
        GraphicsControlBlock c;
        DGifSavedExtensionToGCB(g, i, &c);
        number(c.DisposalMode); number(c.UserInputFlag); number(c.DelayTime); number((uint32_t)c.TransparentColor);
        size_t count = (size_t)d->Width * d->Height;
        if (fwrite(s->RasterBits, 1, count, stdout) != count) return 4;
    }
    if (argc == 3) {
        GifFileType *out = EGifOpenFileName(argv[2], false, &error);
        if (!out) return 3;
        EGifSetGifVersion(out, true);
        out->SWidth = g->SWidth; out->SHeight = g->SHeight;
        out->SColorResolution = g->SColorResolution; out->SBackGroundColor = g->SBackGroundColor;
        out->AspectByte = g->AspectByte;
        if (g->SColorMap) out->SColorMap = GifMakeMapObject(g->SColorMap->ColorCount, g->SColorMap->Colors);
        if (out->SColorMap) out->SColorMap->SortFlag = g->SColorMap->SortFlag;
        for (int i = 0; i < g->ImageCount; ++i) {
            SavedImage *copy = GifMakeSavedImage(out, &g->SavedImages[i]);
            if (!copy) return 3;
            if (!copy->ExtensionBlockCount) for (int j = 0; j < g->SavedImages[i].ExtensionBlockCount; ++j) {
                ExtensionBlock *e = &g->SavedImages[i].ExtensionBlocks[j];
                if (GifAddExtensionBlock(&copy->ExtensionBlockCount, &copy->ExtensionBlocks, e->Function, e->ByteCount, e->Bytes) == GIF_ERROR) return 3;
            }
        }
        for (int i = 0; i < g->ExtensionBlockCount; ++i) {
            ExtensionBlock *e = &g->ExtensionBlocks[i];
            if (GifAddExtensionBlock(&out->ExtensionBlockCount, &out->ExtensionBlocks, e->Function, e->ByteCount, e->Bytes) == GIF_ERROR) return 3;
        }
        if (EGifSpew(out) == GIF_ERROR) return 3;
    }
    DGifCloseFile(g, &error);
    return ferror(stdout) ? 4 : 0;
}

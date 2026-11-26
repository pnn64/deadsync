#include "bridge.h"
#include "jpgd.h"
#include "sjpeg.h"
#include <cstdlib>
#include <cstring>
#include <climits>
#include <cstdint>

#define WUFFS_IMPLEMENTATION
#define WUFFS_CONFIG__STATIC_FUNCTIONS
#define WUFFS_CONFIG__MODULES
#define WUFFS_CONFIG__MODULE__BASE
#define WUFFS_CONFIG__MODULE__JPEG
#include "wuffs-v0.4.c"

void release_output(Output *out) { free(out->data); }

extern "C" EXPORT void release_sjpeg(Output *out) { SjpegFreeBuffer(out->data); }

static void decode_wuffs_jpeg(const uint8_t *data, size_t size, Output *out) {
    wuffs_jpeg__decoder *dec = wuffs_jpeg__decoder__alloc();
    if (!dec) {
        strcpy(out->error, "Wuffs allocation failed");
        return;
    }
    wuffs_base__io_buffer src = wuffs_base__ptr_u8__reader((uint8_t *)data, size, true);
    wuffs_base__image_config config = {};
    wuffs_base__pixel_buffer pixels = {};
    wuffs_base__status status = wuffs_jpeg__decoder__decode_image_config(dec, &config, &src);
    uint8_t *work = nullptr;
    if (!status.repr) {
        out->width = wuffs_base__pixel_config__width(&config.pixcfg);
        out->height = wuffs_base__pixel_config__height(&config.pixcfg);
        bool gray = config.pixcfg.pixel_format().repr == WUFFS_BASE__PIXEL_FORMAT__Y;
        out->channels = gray ? 1 : 3;
        out->precision = 8;
        unsigned format = gray ? WUFFS_BASE__PIXEL_FORMAT__Y : WUFFS_BASE__PIXEL_FORMAT__RGB;
        wuffs_base__pixel_config__set(&config.pixcfg, format, 0, out->width, out->height);
        out->size = (size_t)wuffs_base__pixel_config__pixbuf_len(&config.pixcfg);
        size_t work_len = (size_t)wuffs_jpeg__decoder__workbuf_len(dec).max_incl;
        out->data = (uint8_t *)malloc(out->size);
        work = (uint8_t *)malloc(work_len ? work_len : 1);
        if (out->data && work) {
            status = wuffs_base__pixel_buffer__set_from_slice(
                &pixels, &config.pixcfg, wuffs_base__make_slice_u8(out->data, out->size));
            if (!status.repr)
                status = wuffs_jpeg__decoder__decode_frame(
                    dec, &pixels, &src, WUFFS_BASE__PIXEL_BLEND__SRC,
                    wuffs_base__make_slice_u8(work, work_len), nullptr);
        } else {
            strcpy(out->error, "Wuffs allocation failed");
        }
    }
    if (status.repr) strncpy(out->error, status.repr, sizeof(out->error) - 1);
    free(work);
    free(dec);
}

void decode_jpeg(const uint8_t *bytes, size_t length, int codec, Output *out) {
    if (codec == 1) {
        decode_wuffs_jpeg(bytes, length, out);
        return;
    }
    int width, height, comps;
    if (length <= INT_MAX) {
        out->data = jpgd::decompress_jpeg_image_from_memory(
            bytes, (int)length, &width, &height, &comps, codec == 2 ? 1 : 3);
    }
    if (!out->data) {
        strcpy(out->error, "jpgd rejected the JPEG");
        return;
    }
    out->width = width;
    out->height = height;
    out->channels = codec == 2 ? 1 : 3;
    out->precision = 8;
    out->size = (size_t)width * height * out->channels;
}

void encode_jpeg(const Raster *input, Output *out) {
    bool yuv420 = input->horizontal == 2 && input->vertical == 2;
    bool yuv444 = input->horizontal == 1 && input->vertical == 1;
    if (input->precision != 8 || input->channels != 3 || !(yuv420 || yuv444)
        || (input->flags & (LOSSLESS | ARITHMETIC | PROGRESSIVE))) {
        strcpy(out->error, "sjpeg requires sequential RGB 4:2:0 or 4:4:4");
        return;
    }
    out->size = SjpegEncode(
        (const uint8_t *)input->data, input->width, input->height, 3 * input->width,
        &out->data, (float)input->quality, !!(input->flags & OPTIMIZE),
        yuv420 ? SJPEG_YUV_420 : SJPEG_YUV_444);
    if (!out->size) strcpy(out->error, "sjpeg failed to encode");
}

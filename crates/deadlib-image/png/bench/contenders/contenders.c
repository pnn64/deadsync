#include "contenders.h"

#include <limits.h>
#include <stdlib.h>
#include <string.h>
#include "png.h"
#include "spng.h"

#define STBI_ONLY_PNG
#define STBI_NO_STDIO
#define STB_IMAGE_IMPLEMENTATION
#include "stb_image.h"

#define WUFFS_IMPLEMENTATION
#define WUFFS_CONFIG__STATIC_FUNCTIONS
#define WUFFS_CONFIG__MODULES
#define WUFFS_CONFIG__MODULE__BASE
#define WUFFS_CONFIG__MODULE__ADLER32
#define WUFFS_CONFIG__MODULE__CRC32
#define WUFFS_CONFIG__MODULE__DEFLATE
#define WUFFS_CONFIG__MODULE__PNG
#define WUFFS_CONFIG__MODULE__ZLIB
#include "wuffs-v0.4.c"

void codec_release(Buffer *out) {
    if (out->release) out->release(out->owner);
    else free(out->data);
}

static void decode_wuffs(const uint8_t *data, size_t size, int depth, Buffer *out) {
    wuffs_png__decoder *decoder = wuffs_png__decoder__alloc();
    wuffs_base__io_buffer src = wuffs_base__ptr_u8__reader((uint8_t *)data, size, true);
    wuffs_base__image_config config = {0};
    wuffs_base__pixel_buffer pixels = {0};
    uint8_t *work = NULL;
    if (!decoder) {
        out->error = "Wuffs allocation failed";
        return;
    }
    wuffs_base__status status = wuffs_png__decoder__decode_image_config(decoder, &config, &src);
    if (status.repr) goto done;
    out->width = wuffs_base__pixel_config__width(&config.pixcfg);
    out->height = wuffs_base__pixel_config__height(&config.pixcfg);
    uint32_t format = depth == 16 ? WUFFS_BASE__PIXEL_FORMAT__BGRA_NONPREMUL_4X16LE
                                  : WUFFS_BASE__PIXEL_FORMAT__RGBA_NONPREMUL;
    wuffs_base__pixel_config__set(&config.pixcfg, format, 0, out->width, out->height);
    out->size = (size_t)wuffs_base__pixel_config__pixbuf_len(&config.pixcfg);
    size_t work_size = (size_t)wuffs_png__decoder__workbuf_len(decoder).max_incl;
    out->data = malloc(out->size);
    work = malloc(work_size);
    if (!out->data || !work) {
        out->error = "Wuffs allocation failed";
        goto done;
    }
    status = wuffs_base__pixel_buffer__set_from_slice(
        &pixels, &config.pixcfg, wuffs_base__make_slice_u8(out->data, out->size));
    if (status.repr) goto done;
    status = wuffs_png__decoder__decode_frame(
        decoder, &pixels, &src, WUFFS_BASE__PIXEL_BLEND__SRC,
        wuffs_base__make_slice_u8(work, work_size), NULL);
    if (!status.repr && depth == 16) {
        uint16_t *samples = (uint16_t *)out->data;
        for (size_t i = 0; i < out->size / 2; i += 4) {
            uint16_t blue = samples[i];
            samples[i] = samples[i + 2];
            samples[i + 2] = blue;
        }
    }
done:
    if (status.repr) out->error = status.repr;
    free(work);
    free(decoder);
}

static void decode_spng(const uint8_t *data, size_t size, int depth, Buffer *out) {
    spng_ctx *ctx = spng_ctx_new(0);
    struct spng_ihdr header;
    int format = depth == 16 ? SPNG_FMT_RGBA16 : SPNG_FMT_RGBA8;
    if (!ctx) {
        out->error = "spng context allocation failed";
        return;
    }
    int error = spng_set_png_buffer(ctx, data, size);
    if (!error) error = spng_get_ihdr(ctx, &header);
    if (!error) error = spng_decoded_image_size(ctx, format, &out->size);
    if (!error) {
        out->width = header.width;
        out->height = header.height;
        out->data = malloc(out->size);
        error = out->data ? spng_decode_image(ctx, out->data, out->size, format, SPNG_DECODE_TRNS)
                          : SPNG_EMEM;
    }
    if (error) out->error = spng_strerror(error);
    spng_ctx_free(ctx);
}

typedef struct {
    const uint8_t *data;
    size_t remaining, capacity;
    Buffer *out;
} Io;

static void png_fail(png_structp png, png_const_charp message) {
    Buffer *out = png_get_error_ptr(png);
    (void)message;
    out->error = "libpng rejected input or allocation failed";
    png_longjmp(png, 1);
}

static void png_warn(png_structp png, png_const_charp message) {
    (void)png;
    (void)message;
}

static void read_png(png_structp png, png_bytep output, size_t count) {
    Io *io = png_get_io_ptr(png);
    if (count > io->remaining) png_error(png, "truncated input");
    memcpy(output, io->data, count);
    io->data += count;
    io->remaining -= count;
}

static void decode_libpng(const uint8_t *data, size_t size, int depth, Buffer *out) {
    png_structp png = png_create_read_struct(PNG_LIBPNG_VER_STRING, out, png_fail, png_warn);
    png_infop info = png ? png_create_info_struct(png) : NULL;
    if (!info) {
        out->error = "libpng context allocation failed";
    } else if (!setjmp(png_jmpbuf(png))) {
        Io io = {data, size, 0, out};
        png_set_read_fn(png, &io, read_png);
        png_read_info(png, info);
        out->width = png_get_image_width(png, info);
        out->height = png_get_image_height(png, info);
        int color = png_get_color_type(png, info);
        int alpha = (color & PNG_COLOR_MASK_ALPHA) || png_get_valid(png, info, PNG_INFO_tRNS);
        png_set_expand(png);
        if (depth == 8) png_set_strip_16(png);
        else png_set_swap(png); // The 16-bit benchmark uses little-endian samples.
        if (!(color & PNG_COLOR_MASK_COLOR)) png_set_gray_to_rgb(png);
        if (!alpha) png_set_add_alpha(png, depth == 8 ? 255 : 65535, PNG_FILLER_AFTER);
        int passes = png_set_interlace_handling(png);
        png_read_update_info(png, info);
        size_t row_bytes = png_get_rowbytes(png, info);
        out->size = row_bytes * out->height;
        out->data = malloc(out->size);
        if (!out->data) png_error(png, "allocation failed");
        for (int pass = 0; pass < passes; ++pass) {
            for (uint32_t y = 0; y < out->height; ++y) {
                png_read_row(png, out->data + y * row_bytes, NULL);
            }
        }
        png_read_end(png, info);
    }
    png_destroy_read_struct(&png, &info, NULL);
}

static void decode_stb(const uint8_t *data, size_t size, int depth, Buffer *out) {
    int width, height;
    if (size > INT_MAX) {
        out->error = "stb input too large";
        return;
    }
    if (depth == 16) out->data = (uint8_t *)stbi_load_16_from_memory(data, (int)size, &width, &height, NULL, 4);
    else out->data = stbi_load_from_memory(data, (int)size, &width, &height, NULL, 4);
    if (!out->data) {
        out->error = stbi_failure_reason();
        return;
    }
    out->width = (uint32_t)width;
    out->height = (uint32_t)height;
    out->size = (size_t)width * height * (depth == 16 ? 8 : 4);
}

void codec_decode(int codec, const uint8_t *data, size_t size, int depth, Buffer *out) {
    switch (codec) {
        case 0: decode_wuffs(data, size, depth, out); break;
        case 1: decode_spng(data, size, depth, out); break;
        case 2: decode_libpng(data, size, depth, out); break;
        case 3: decode_stb(data, size, depth, out); break;
        case 4: fast_decode(data, size, out); break;
        default: out->error = "unknown decoder";
    }
}

static void write_png(png_structp png, png_bytep data, size_t count) {
    Io *io = png_get_io_ptr(png);
    Buffer *out = io->out;
    if (count > io->capacity - out->size) {
        size_t size = out->size + count;
        size_t capacity = io->capacity ? io->capacity * 2 : 256;
        if (size > capacity) capacity = size;
        uint8_t *grown = realloc(out->data, capacity);
        if (!grown) png_error(png, "allocation failed");
        out->data = grown;
        io->capacity = capacity;
    }
    memcpy(out->data + out->size, data, count);
    out->size += count;
}

static void encode_libpng(const Raster *input, int level, Buffer *out) {
    png_structp png = png_create_write_struct(PNG_LIBPNG_VER_STRING, out, png_fail, png_warn);
    png_infop info = png ? png_create_info_struct(png) : NULL;
    if (!info) {
        out->error = "libpng context allocation failed";
    } else if (!setjmp(png_jmpbuf(png))) {
        Io io = {0};
        io.out = out;
        png_set_write_fn(png, &io, write_png, NULL);
        png_set_IHDR(png, info, input->width, input->height, input->depth, input->color,
                     input->interlaced, PNG_COMPRESSION_TYPE_BASE, PNG_FILTER_TYPE_BASE);
        if (input->palette_len) {
            png_color palette[256];
            for (size_t i = 0; i < input->palette_len; ++i) {
                palette[i] = (png_color){input->palette[3*i], input->palette[3*i+1], input->palette[3*i+2]};
            }
            png_set_PLTE(png, info, palette, (int)input->palette_len);
        }
        png_set_compression_level(png, level);
        png_write_info(png, info);
        int passes = png_set_interlace_handling(png);
        for (int pass = 0; pass < passes; ++pass) {
            for (uint32_t y = 0; y < input->height; ++y) {
                png_write_row(png, input->pixels + y * input->row_bytes);
            }
        }
        png_write_end(png, info);
    }
    png_destroy_write_struct(&png, &info);
}

static void encode_spng(const Raster *input, int level, Buffer *out) {
    spng_ctx *ctx = spng_ctx_new(SPNG_CTX_ENCODER);
    struct spng_ihdr header = {
        input->width, input->height, input->depth, input->color, 0, 0, input->interlaced
    };
    if (!ctx) {
        out->error = "spng context allocation failed";
        return;
    }
    int err = spng_set_ihdr(ctx, &header);
    if (!err) err = spng_set_option(ctx, SPNG_ENCODE_TO_BUFFER, 1);
    if (!err) err = spng_set_option(ctx, SPNG_IMG_COMPRESSION_LEVEL, level);
    if (!err && input->palette_len) {
        struct spng_plte palette = {0};
        palette.n_entries = (uint32_t)input->palette_len;
        for (size_t i = 0; i < input->palette_len; ++i) {
            palette.entries[i].red = input->palette[3*i];
            palette.entries[i].green = input->palette[3*i+1];
            palette.entries[i].blue = input->palette[3*i+2];
        }
        err = spng_set_plte(ctx, &palette);
    }
    if (!err) err = spng_encode_image(ctx, input->pixels, input->row_bytes * input->height,
                                       SPNG_FMT_RAW, SPNG_ENCODE_FINALIZE);
    if (!err) out->data = spng_get_png_buffer(ctx, &out->size, &err);
    if (err) out->error = spng_strerror(err);
    spng_ctx_free(ctx);
}

void codec_encode(int codec, const Raster *input, int level, Buffer *out) {
    switch (codec) {
        case 1: encode_spng(input, level, out); break;
        case 2: encode_libpng(input, level, out); break;
        case 4: case 5: fast_encode(codec, input, level, out); break;
        default: out->error = "unknown encoder";
    }
}

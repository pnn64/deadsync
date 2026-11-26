#include "bridge.h"

#include <setjmp.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "jpeglib.h"
#ifdef JPEG_LI
#include "lib/jpegli/decode.h"
#include "lib/jpegli/encode.h"
#define J(name) jpegli_##name
#else
#define J(name) jpeg_##name
#endif

typedef struct {
    struct jpeg_error_mgr base;
    jmp_buf jump;
    Output *out;
} Errors;

static void failure(j_common_ptr info) {
    Errors *error = (Errors *)info->err;
    (*info->err->format_message)(info, error->out->error);
    longjmp(error->jump, 1);
}

static void silence(j_common_ptr info, int level) {
    (void)info;
    (void)level;
}

static void setup(Errors *errors, Output *out) {
    J(std_error)(&errors->base);
    errors->out = out;
    errors->base.error_exit = failure;
    errors->base.emit_message = silence;
}

void release_output(Output *out) { free(out->data); }

typedef struct {
    struct jpeg_decompress_struct jpeg;
    Errors errors;
    uint8_t *scratch;
} Decoder;

static JDIMENSION read_row(j_decompress_ptr jpeg, uint8_t *dest, unsigned precision) {
#ifdef HIGH_PRECISION
    if (precision > 12) {
        J16SAMPROW row = (J16SAMPROW)dest;
        return jpeg16_read_scanlines(jpeg, &row, 1);
    }
    if (precision > 8) {
        J12SAMPROW row = (J12SAMPROW)dest;
        return jpeg12_read_scanlines(jpeg, &row, 1);
    }
#else
    (void)precision;
#endif
    JSAMPROW row = dest;
    return J(read_scanlines)(jpeg, &row, 1);
}

static unsigned subtractive(unsigned component, unsigned black, unsigned max, int inverted) {
    if (!inverted) {
        component = max - component;
        black = max - black;
    }
    return (component * black + max / 2) / max;
}

void decode_jpeg(const uint8_t *bytes, size_t length, int fast, Output *out) {
    Decoder *ctx = (Decoder *)calloc(1, sizeof(*ctx));
    if (!ctx) {
        strcpy(out->error, "decoder allocation failed");
        return;
    }
    j_decompress_ptr jpeg = &ctx->jpeg;
    int cmyk, channels;
    size_t sample, stride;
    setup(&ctx->errors, out);
    jpeg->err = &ctx->errors.base;
    if (setjmp(ctx->errors.jump)) goto done;
    J(create_decompress)(jpeg);
    J(mem_src)(jpeg, bytes, (unsigned long)length);
    J(read_header)(jpeg, TRUE);
#ifndef HIGH_PRECISION
    if (jpeg->data_precision > 8) {
        strcpy(out->error, "high-precision decoder unavailable");
        goto done;
    }
#endif
    cmyk = jpeg->jpeg_color_space == JCS_CMYK || jpeg->jpeg_color_space == JCS_YCCK;
    channels = jpeg->num_components == 1 ? 1 : 3;
    jpeg->out_color_space = cmyk ? JCS_CMYK : channels == 1 ? JCS_GRAYSCALE : JCS_RGB;
    if (fast) {
        jpeg->do_fancy_upsampling = FALSE;
        jpeg->dct_method = JDCT_FASTEST;
    }
    J(start_decompress)(jpeg);
    out->precision = jpeg->data_precision;
    out->width = jpeg->output_width;
    out->height = jpeg->output_height;
    out->channels = channels;
    sample = out->precision > 8 ? 2 : 1;
    stride = out->width * channels * sample;
    out->size = stride * out->height;
    out->data = (uint8_t *)malloc(out->size);
    if (!out->data) {
        strcpy(out->error, "pixel allocation failed");
        goto done;
    }
    if (cmyk) {
        size_t raw_len = out->width * 4 * sample;
        ctx->scratch = (uint8_t *)malloc(raw_len);
        if (!ctx->scratch) {
            strcpy(out->error, "CMYK row allocation failed");
            goto done;
        }
        int inverted = jpeg->saw_Adobe_marker;
        unsigned max = (1U << out->precision) - 1;
        while (jpeg->output_scanline < out->height) {
            uint8_t *dest = out->data + jpeg->output_scanline * stride;
            if (read_row(jpeg, ctx->scratch, out->precision) != 1) goto short_read;
            if (sample == 1) {
                for (size_t x = 0; x < out->width; ++x) {
                    const uint8_t *ink = ctx->scratch + x * 4;
                    for (unsigned i = 0; i < 3; ++i)
                        dest[3*x+i] = (uint8_t)subtractive(ink[i], ink[3], 255, inverted);
                }
            } else {
                const uint16_t *row = (const uint16_t *)ctx->scratch;
                uint16_t *rgb = (uint16_t *)dest;
                for (size_t x = 0; x < out->width; ++x) {
                    const uint16_t *ink = row + x * 4;
                    for (unsigned i = 0; i < 3; ++i)
                        rgb[3*x+i] = (uint16_t)subtractive(ink[i], ink[3], max, inverted);
                }
            }
        }
    } else {
        while (jpeg->output_scanline < out->height) {
            uint8_t *dest = out->data + jpeg->output_scanline * stride;
            if (read_row(jpeg, dest, out->precision) != 1) goto short_read;
        }
    }
    J(finish_decompress)(jpeg);
    goto done;
short_read:
    strcpy(out->error, "decoder returned a short raster");
done:
    J(destroy_decompress)(jpeg);
    free(ctx->scratch);
    free(ctx);
}

typedef struct {
    struct jpeg_compress_struct jpeg;
    Errors errors;
    unsigned long size;
} Encoder;

void encode_jpeg(const Raster *input, Output *out) {
    Encoder *ctx = (Encoder *)calloc(1, sizeof(*ctx));
    if (!ctx) {
        strcpy(out->error, "encoder allocation failed");
        return;
    }
    j_compress_ptr jpeg = &ctx->jpeg;
    size_t stride;
    setup(&ctx->errors, out);
    jpeg->err = &ctx->errors.base;
    if (setjmp(ctx->errors.jump)) goto done;
    J(create_compress)(jpeg);
    J(mem_dest)(jpeg, &out->data, &ctx->size);
    jpeg->image_width = input->width;
    jpeg->image_height = input->height;
    jpeg->input_components = input->channels;
    jpeg->in_color_space = input->channels == 1 ? JCS_GRAYSCALE : JCS_RGB;
#ifdef MOZ_JPEG
    if (!(input->flags & NATIVE_PROFILE))
        jpeg_c_set_int_param(jpeg, JINT_COMPRESS_PROFILE, JCP_FASTEST);
#endif
    J(set_defaults)(jpeg);
    jpeg->data_precision = input->precision;
    J(set_quality)(jpeg, input->quality, TRUE);
    if (input->channels == 3) {
        jpeg->comp_info[0].h_samp_factor = input->horizontal;
        jpeg->comp_info[0].v_samp_factor = input->vertical;
        jpeg->comp_info[1].h_samp_factor = jpeg->comp_info[1].v_samp_factor = 1;
        jpeg->comp_info[2].h_samp_factor = jpeg->comp_info[2].v_samp_factor = 1;
    }
    jpeg->optimize_coding = !!(input->flags & OPTIMIZE);
    jpeg->arith_code = !!(input->flags & ARITHMETIC);
#ifdef JPEG_LI
    jpegli_set_progressive_level(jpeg, input->flags & PROGRESSIVE ? 2 : 0);
#else
    if (input->flags & PROGRESSIVE) J(simple_progression)(jpeg);
    else {
        jpeg->scan_info = NULL;
        jpeg->num_scans = 0;
    }
#endif
#ifdef HIGH_PRECISION
    if (input->flags & LOSSLESS) jpeg_enable_lossless(jpeg, 1, 0);
#else
    if (input->flags & LOSSLESS || input->precision != 8) {
        strcpy(out->error, "encoder precision or process unavailable");
        goto done;
    }
#endif
    J(start_compress)(jpeg, TRUE);
    stride = (size_t)input->width * input->channels * (input->precision > 8 ? 2 : 1);
    while (jpeg->next_scanline < input->height) {
        const uint8_t *data = (const uint8_t *)input->data + jpeg->next_scanline * stride;
        JDIMENSION sent = 0;
#ifdef HIGH_PRECISION
        if (input->precision > 12) {
            J16SAMPROW row = (J16SAMPROW)data;
            sent = jpeg16_write_scanlines(jpeg, &row, 1);
        } else if (input->precision > 8) {
            J12SAMPROW row = (J12SAMPROW)data;
            sent = jpeg12_write_scanlines(jpeg, &row, 1);
        } else
#endif
        {
            JSAMPROW row = (JSAMPROW)data;
            sent = J(write_scanlines)(jpeg, &row, 1);
        }
        if (sent != 1) {
            strcpy(out->error, "encoder accepted a short raster");
            goto done;
        }
    }
    J(finish_compress)(jpeg);
    out->size = ctx->size;
done:
    J(destroy_compress)(jpeg);
    free(ctx);
}

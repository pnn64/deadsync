#pragma once
#include <stddef.h>
#include <stdint.h>

struct Output {
    uint8_t *data;
    size_t size;
    uint32_t width, height;
    void *owner;
    void (*release)(void *);
    char error[160];
};

enum { STB, WUFFS, SDL, WIC, NSBMP, FFMPEG };
#define API extern "C" __declspec(dllexport)
API int init_bmp();
API void decode_bmp(unsigned codec, const uint8_t *data, size_t len, Output *out);
API void encode_bmp(unsigned codec, const uint8_t *data, unsigned w, unsigned h,
                    unsigned channels, Output *out);

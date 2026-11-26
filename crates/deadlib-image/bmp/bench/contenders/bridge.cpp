#include "bridge.h"
#include <climits>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <windows.h>
#include <shlwapi.h>
#include <wincodec.h>
#include <wrl/client.h>
#include <SDL3/SDL.h>
extern "C" {
#include "libnsbmp.h"
}

#define STBI_ONLY_BMP
#define STBI_NO_STDIO
#define STB_IMAGE_IMPLEMENTATION
#include "stb_image.h"
#define STBI_WRITE_NO_STDIO
#define STB_IMAGE_WRITE_IMPLEMENTATION
#include "stb_image_write.h"
#define WUFFS_IMPLEMENTATION
#define WUFFS_CONFIG__STATIC_FUNCTIONS
#define WUFFS_CONFIG__MODULES
#define WUFFS_CONFIG__MODULE__BASE
#define WUFFS_CONFIG__MODULE__BMP
#include "wuffs-v0.4.c"

using Microsoft::WRL::ComPtr;
static ComPtr<IWICImagingFactory> wic;

static void error(Output *o, const char *message) {
    snprintf(o->error, sizeof(o->error), "%s", message ? message : "codec error");
}
static void wic_error(Output *o, HRESULT hr) {
    snprintf(o->error, sizeof(o->error), "WIC error 0x%08lx", (unsigned long)hr);
}
static void own(Output *o, uint8_t *data, void *owner, void (*drop)(void *)) {
    o->data = data;
    o->owner = owner;
    o->release = drop;
}
static bool pixels(Output *o, unsigned w, unsigned h) {
    if (!w || !h || (size_t)w * h > (1u << 26)) {
        error(o, "raster exceeds benchmark size limit");
        return false;
    }
    o->width = w;
    o->height = h;
    o->size = (size_t)w * h * 4;
    return true;
}
static bool allocate(Output *o) {
    auto p = (uint8_t *)malloc(o->size);
    if (!p) { error(o, "allocation failed"); return false; }
    own(o, p, p, free);
    return true;
}

int init_bmp() {
    if (FAILED(CoInitializeEx(nullptr, COINIT_MULTITHREADED))) return 0;
    return SUCCEEDED(CoCreateInstance(CLSID_WICImagingFactory2, nullptr, CLSCTX_INPROC_SERVER,
                                     IID_PPV_ARGS(&wic)));
}

static void decode_wuffs(const uint8_t *data, size_t len, Output *o) {
    auto d = wuffs_bmp__decoder__alloc();
    if (!d) { error(o, "Wuffs allocation failed"); return; }
    auto src = wuffs_base__ptr_u8__reader((uint8_t *)data, len, true);
    wuffs_base__image_config config = {};
    wuffs_base__pixel_buffer dest = {};
    auto st = wuffs_bmp__decoder__decode_image_config(d, &config, &src);
    if (!st.repr && pixels(o, config.pixcfg.width(), config.pixcfg.height()) && allocate(o)) {
        config.pixcfg.set(WUFFS_BASE__PIXEL_FORMAT__RGBA_NONPREMUL, 0, o->width, o->height);
        st = wuffs_base__pixel_buffer__set_from_slice(&dest, &config.pixcfg,
                                                     wuffs_base__make_slice_u8(o->data, o->size));
        size_t n = (size_t)wuffs_bmp__decoder__workbuf_len(d).max_incl;
        auto work = n ? (uint8_t *)malloc(n) : nullptr;
        if (n && !work) error(o, "Wuffs work allocation failed");
        else if (!st.repr) st = wuffs_bmp__decoder__decode_frame(d, &dest, &src,
            WUFFS_BASE__PIXEL_BLEND__SRC, wuffs_base__make_slice_u8(work, n), nullptr);
        free(work);
    }
    if (st.repr) error(o, st.repr);
    free(d);
}

static void drop_surface(void *surface) { SDL_DestroySurface((SDL_Surface *)surface); }
static void decode_sdl(const uint8_t *data, size_t len, Output *o) {
    SDL_IOStream *s = SDL_IOFromConstMem(data, len);
    SDL_Surface *bmp = s ? SDL_LoadBMP_IO(s, true) : nullptr;
    if (!bmp) { error(o, SDL_GetError()); return; }
    if (bmp->format != SDL_PIXELFORMAT_RGBA32) {
        auto converted = SDL_ConvertSurface(bmp, SDL_PIXELFORMAT_RGBA32);
        SDL_DestroySurface(bmp);
        bmp = converted;
    }
    if (!bmp) { error(o, SDL_GetError()); return; }
    if (pixels(o, bmp->w, bmp->h)) {
        if (bmp->pitch == bmp->w * 4) {
            own(o, (uint8_t *)bmp->pixels, bmp, drop_surface);
            return;
        }
        if (allocate(o)) for (int y = 0; y < bmp->h; ++y)
            memcpy(o->data + (size_t)y * bmp->w * 4,
                   (uint8_t *)bmp->pixels + (size_t)y * bmp->pitch, bmp->w * 4);
    }
    SDL_DestroySurface(bmp);
}

static void decode_wic(const uint8_t *data, size_t len, Output *o) {
    if (len > UINT_MAX) { error(o, "WIC input exceeds UINT"); return; }
    ComPtr<IWICStream> stream;
    ComPtr<IWICBitmapDecoder> decoder;
    ComPtr<IWICBitmapFrameDecode> frame;
    ComPtr<IWICFormatConverter> converter;
    auto hr = wic->CreateStream(&stream);
    if (SUCCEEDED(hr)) hr = stream->InitializeFromMemory((BYTE *)data, (DWORD)len);
    if (SUCCEEDED(hr)) hr = wic->CreateDecoder(GUID_ContainerFormatBmp, nullptr, &decoder);
    if (SUCCEEDED(hr)) hr = decoder->Initialize(stream.Get(), WICDecodeMetadataCacheOnDemand);
    if (SUCCEEDED(hr)) hr = decoder->GetFrame(0, &frame);
    if (SUCCEEDED(hr)) hr = wic->CreateFormatConverter(&converter);
    if (SUCCEEDED(hr)) hr = converter->Initialize(frame.Get(), GUID_WICPixelFormat32bppRGBA,
        WICBitmapDitherTypeNone, nullptr, 0.0, WICBitmapPaletteTypeCustom);
    UINT w, h;
    if (SUCCEEDED(hr)) hr = converter->GetSize(&w, &h);
    if (SUCCEEDED(hr) && pixels(o, w, h) && allocate(o))
        hr = converter->CopyPixels(nullptr, w * 4, (UINT)o->size, o->data);
    if (FAILED(hr)) wic_error(o, hr);
}

static void *ns_allocate(int w, int h, unsigned state) {
    if (w <= 0 || h <= 0 || (size_t)w * h > (1u << 26)) return nullptr;
    return state & BMP_CLEAR_MEMORY ? calloc((size_t)w * h, 4) : malloc((size_t)w * h * 4);
}
static unsigned char *ns_buffer(void *p) { return (unsigned char *)p; }
static void decode_ns(const uint8_t *data, size_t len, Output *o) {
    bmp_bitmap_callback_vt callbacks = {ns_allocate, free, ns_buffer};
    bmp_image bmp = {};
    bmp_result status = bmp_create(&bmp, &callbacks);
    if (status == BMP_OK) status = bmp_analyse(&bmp, len, (uint8_t *)data);
    if (status == BMP_OK) status = bmp_decode(&bmp);
    if (status == BMP_OK && bmp.bitmap && pixels(o, bmp.width, bmp.height)) {
        own(o, (uint8_t *)bmp.bitmap, bmp.bitmap, free);
        bmp.bitmap = nullptr;
    } else if (!o->error[0]) {
        snprintf(o->error, sizeof(o->error), "libnsbmp result %d", (int)status);
    }
    bmp_finalise(&bmp);
}

void decode_bmp(unsigned codec, const uint8_t *data, size_t len, Output *o) {
    switch (codec) {
        case STB: {
            int w, h, components;
            auto p = len <= INT_MAX ? stbi_load_from_memory(data, (int)len, &w, &h, &components, 4) : nullptr;
            if (!p) error(o, stbi_failure_reason());
            else if (pixels(o, w, h)) own(o, p, p, stbi_image_free);
            else stbi_image_free(p);
            break;
        }
        case WUFFS: decode_wuffs(data, len, o); break;
        case SDL: decode_sdl(data, len, o); break;
        case WIC: decode_wic(data, len, o); break;
        case NSBMP: decode_ns(data, len, o); break;
        default: error(o, "decode unavailable");
    }
}

struct StbSink { Output *out; size_t capacity; };
static void stb_append(void *context, void *data, int n) {
    auto &s = *(StbSink *)context;
    auto o = s.out;
    if (o->error[0] || n <= 0) return;
    if (o->size + n > s.capacity) {
        size_t capacity = (o->size + n) * 2 + 1024;
        auto p = (uint8_t *)realloc(o->data, capacity);
        if (!p) { error(o, "allocation failed"); return; }
        own(o, p, p, free);
        s.capacity = capacity;
    }
    memcpy(o->data + o->size, data, n);
    o->size += n;
}

static void encode_sdl(const uint8_t *data, unsigned w, unsigned h, unsigned n, Output *o) {
    auto surface = SDL_CreateSurfaceFrom(w, h, n == 3 ? SDL_PIXELFORMAT_RGB24 : SDL_PIXELFORMAT_RGBA32,
                                         (void *)data, w * n);
    auto stream = SDL_IOFromDynamicMem();
    if (surface && stream && SDL_SaveBMP_IO(surface, stream, false)) {
        auto len = SDL_GetIOSize(stream);
        auto props = SDL_GetIOProperties(stream);
        auto p = (uint8_t *)SDL_GetPointerProperty(props, SDL_PROP_IOSTREAM_DYNAMIC_MEMORY_POINTER, nullptr);
        if (len > 0 && p && SDL_SetPointerProperty(props, SDL_PROP_IOSTREAM_DYNAMIC_MEMORY_POINTER, nullptr)) {
            o->size = (size_t)len;
            own(o, p, p, SDL_free);
        }
    }
    if (!o->data) error(o, SDL_GetError());
    if (stream) SDL_CloseIO(stream);
    SDL_DestroySurface(surface);
}

static void drop_hglobal(void *owner) {
    GlobalUnlock(owner);
    GlobalFree(owner);
}
static void encode_wic(const uint8_t *data, unsigned w, unsigned h, unsigned n, Output *o) {
    ComPtr<IStream> stream;
    ComPtr<IWICBitmapEncoder> encoder;
    ComPtr<IWICBitmapFrameEncode> frame;
    ComPtr<IPropertyBag2> options;
    ComPtr<IWICBitmap> bitmap;
    auto hr = CreateStreamOnHGlobal(nullptr, FALSE, &stream);
    if (SUCCEEDED(hr)) hr = wic->CreateEncoder(GUID_ContainerFormatBmp, nullptr, &encoder);
    if (SUCCEEDED(hr)) hr = encoder->Initialize(stream.Get(), WICBitmapEncoderNoCache);
    if (SUCCEEDED(hr)) hr = encoder->CreateNewFrame(&frame, &options);
    if (SUCCEEDED(hr) && n == 4) {
        PROPBAG2 option = {};
        option.pstrName = const_cast<wchar_t *>(L"EnableV5Header32bppBGRA");
        VARIANT value = {};
        value.vt = VT_BOOL;
        value.boolVal = VARIANT_TRUE;
        hr = options->Write(1, &option, &value);
    }
    if (SUCCEEDED(hr)) hr = frame->Initialize(options.Get());
    if (SUCCEEDED(hr)) hr = frame->SetSize(w, h);
    WICPixelFormatGUID fmt = n == 3 ? GUID_WICPixelFormat24bppBGR : GUID_WICPixelFormat32bppBGRA;
    if (SUCCEEDED(hr)) hr = frame->SetPixelFormat(&fmt);
    if (SUCCEEDED(hr)) hr = wic->CreateBitmapFromMemory(w, h, n == 3 ? GUID_WICPixelFormat24bppRGB :
          GUID_WICPixelFormat32bppRGBA, w * n, w * h * n, (BYTE *)data, &bitmap);
    if (SUCCEEDED(hr)) hr = frame->WriteSource(bitmap.Get(), nullptr);
    if (SUCCEEDED(hr)) hr = frame->Commit();
    if (SUCCEEDED(hr)) hr = encoder->Commit();
    STATSTG stat = {};
    if (SUCCEEDED(hr)) hr = stream->Stat(&stat, STATFLAG_NONAME);
    HGLOBAL owner = nullptr;
    if (stream) GetHGlobalFromStream(stream.Get(), &owner);
    if (SUCCEEDED(hr) && owner) {
        auto p = (uint8_t *)GlobalLock(owner);
        if (p) { o->size = (size_t)stat.cbSize.QuadPart; own(o, p, owner, drop_hglobal); }
    }
    frame.Reset();
    encoder.Reset();
    stream.Reset();
    if (!o->data) {
        if (owner) GlobalFree(owner);
        wic_error(o, FAILED(hr) ? hr : E_FAIL);
    }
}

void encode_bmp(unsigned codec, const uint8_t *data, unsigned w, unsigned h, unsigned n, Output *o) {
    switch (codec) {
        case STB: {
            StbSink sink = {o, 0};
            if (!stbi_write_bmp_to_func(stb_append, &sink, w, h, n, data)) error(o, "stb encoder failed");
            break;
        }
        case SDL: encode_sdl(data, w, h, n, o); break;
        case WIC: encode_wic(data, w, h, n, o); break;
        default: error(o, "encoder unavailable");
    }
}

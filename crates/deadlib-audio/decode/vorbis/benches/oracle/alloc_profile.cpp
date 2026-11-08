#define DEADLIB_ALLOC_IMPLEMENTATION
#include "alloc_profile.h"
#include <cstddef>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <cassert>
#include <new>

namespace {
struct alignas(std::max_align_t) Header { size_t bytes; };
uint64_t live = 0, base = 0, peak = 0, operations = 0, requested = 0;
bool active = false;
void added(size_t bytes, size_t previous) {
    live = live - previous + bytes;
    if (active) {
        ++operations; requested += bytes;
        if (live > peak) peak = live;
    }
}
}
extern "C" void *dlab_malloc(size_t bytes) {
    if (bytes > std::numeric_limits<size_t>::max() - sizeof(Header)) return nullptr;
    auto *header = static_cast<Header*>(std::malloc(sizeof(Header) + bytes));
    if (!header) return nullptr;
    new (header) Header{bytes}; added(bytes, 0);
    return header + 1;
}
extern "C" void *dlab_calloc(size_t count, size_t bytes) {
    if (bytes != 0 && count > std::numeric_limits<size_t>::max() / bytes) return nullptr;
    size_t size = count * bytes;
    void *pointer = dlab_malloc(size);
    if (pointer) std::memset(pointer, 0, size);
    return pointer;
}
extern "C" void dlab_free(void *pointer) {
    if (!pointer) return;
    auto *header = static_cast<Header*>(pointer) - 1;
    live -= header->bytes;
    std::free(header);
}
extern "C" void *dlab_realloc(void *pointer, size_t bytes) {
    if (!pointer) return dlab_malloc(bytes);
    if (bytes == 0) { dlab_free(pointer); return nullptr; }
    if (bytes > std::numeric_limits<size_t>::max() - sizeof(Header)) return nullptr;
    auto *header = static_cast<Header*>(pointer) - 1;
    size_t previous = header->bytes;
    auto *next = static_cast<Header*>(std::realloc(header, sizeof(Header) + bytes));
    if (!next) return nullptr;
    next->bytes = bytes; added(bytes, previous);
    return next + 1;
}
extern "C" void dlab_begin(void) {
    assert(!active); base = peak = live; operations = requested = 0; active = true;
}
extern "C" dlab_stats dlab_end(void) {
    assert(active); active = false;
    return {operations, requested, peak - base, static_cast<int64_t>(live) - static_cast<int64_t>(base)};
}
extern "C" void dlab_self_test(void) {
    dlab_begin();
    auto *pointer = static_cast<unsigned char*>(dlab_calloc(17, 1));
    if (!pointer || reinterpret_cast<uintptr_t>(pointer) % alignof(std::max_align_t) != 0) std::abort();
    for (size_t i = 0; i < 17; ++i) if (pointer[i] != 0) std::abort();
    pointer = static_cast<unsigned char*>(dlab_realloc(pointer, 41));
    if (!pointer) std::abort();
    dlab_free(pointer);
    if (dlab_calloc(std::numeric_limits<size_t>::max(), 2) != nullptr) std::abort();
    auto stats = dlab_end();
    if (stats.operations != 2 || stats.requested_bytes != 58 || stats.peak_extra_bytes != 41 || stats.retained_delta_bytes != 0) std::abort();
}

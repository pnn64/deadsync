#if defined(_MSC_VER)
#include <intrin.h>
__declspec(noinline) void deadlib_flac_bench_opaque(const void* pointer) {
    (void)pointer;
    _ReadWriteBarrier();
}
#else
void deadlib_flac_bench_opaque(const void* pointer) {
    __asm__ volatile("" : : "g"(pointer) : "memory");
}
#endif

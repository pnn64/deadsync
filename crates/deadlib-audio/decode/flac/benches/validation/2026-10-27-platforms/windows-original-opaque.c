// A non-LTO boundary makes every PCM plane escape the optimizing caller.
// It has the same purpose as the GNU memory barrier in the Linux driver.
#include <intrin.h>
__declspec(noinline) void deadlib_flac_bench_opaque(const void* pointer) {
    (void)pointer;
    _ReadWriteBarrier();
}

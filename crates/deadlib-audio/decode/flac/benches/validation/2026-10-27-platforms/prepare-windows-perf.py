from pathlib import Path
source=Path('crates/deadlib-audio/decode/flac/benches/oracle/benchmark.cpp').read_text()
source=source.replace('#include <libavcodec/avcodec.h>\n','').replace('#include <libavutil/samplefmt.h>\n','')
needle='static void opaque(const void* pointer){asm volatile(""::"g"(pointer):"memory");}'
assert needle in source
source=source.replace(needle,'extern "C" void deadlib_flac_bench_opaque(const void* pointer);\nstatic void opaque(const void* pointer){deadlib_flac_bench_opaque(pointer);}')
start=source.index('    }else if(mode=="ffmpeg"){')
end=source.index('    }else require(false,"unknown mode");',start)
source=source[:start]+source[end:]
source=source.replace('#include <chrono>','#include <cstdint>\n#include <chrono>')
print('*** Begin Patch\n*** Add File: target/deadlib-flac-reference/windows-perf/main.cpp')
for line in source.splitlines():print('+'+line)
print('*** End Patch')

// Test-only resident-input benchmark against the pinned upstream C sources.
#include <vorbis/vorbisfile.h>
#include <algorithm>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iterator>
#include <limits>
#include <string>
#include <vector>

using Clock = std::chrono::steady_clock;
static void require(bool condition, const char* message) {
    if (!condition) { std::fprintf(stderr, "%s\n", message); std::exit(1); }
}
struct Source {
    const std::vector<char>& bytes;
    size_t position = 0, cap;
    uint64_t reads = 0, consumed = 0;
};
static size_t read_source(void* output, size_t size, size_t count, void* opaque) {
    auto& source = *static_cast<Source*>(opaque);
    if (size == 0) return 0;
    size_t items = std::min({count, (source.bytes.size() - source.position) / size, source.cap / size});
    size_t bytes = items * size;
    std::memcpy(output, source.bytes.data() + source.position, bytes);
    source.position += bytes; source.consumed += bytes; source.reads++;
    return items;
}
struct Measurement { uint64_t ns = 0, samples = 0, reads = 0, bytes = 0; };
static volatile double sink = 0;
static Measurement run(const std::vector<char>& bytes, const std::string& phase) {
    Source source{bytes, 0, phase == "stream4096" ? 4096 : std::numeric_limits<size_t>::max()};
    OggVorbis_File file{};
    ov_callbacks callbacks{read_source, nullptr, nullptr, nullptr};
    Measurement measured, first;
    bool started = false;
    const auto begin = Clock::now();
    require(ov_open_callbacks(&source, &file, nullptr, 0, callbacks) == 0, "ov_open_callbacks");
    int link = 0;
    float** pcm = nullptr;
    for (;;) {
        long frames = ov_read_float(&file, &pcm, 4096, &link);
        require(frames >= 0, "ov_read_float");
        if (frames == 0) break;
        require(link == 0, "single-stream benchmark input");
        int channels = ov_info(&file, -1)->channels;
        for (int ch = 0; ch < channels; ++ch) sink += pcm[ch][0] + pcm[ch][frames - 1];
        measured.samples += static_cast<uint64_t>(frames) * channels;
        if (!started) {
            first.ns = std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now() - begin).count();
            first.reads = source.reads; first.bytes = source.consumed; started = true;
        }
        if (phase == "startup") break;
    }
    ov_clear(&file);
    measured.ns = std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now() - begin).count();
    measured.reads = source.reads; measured.bytes = source.consumed;
    if (phase == "startup") {
        require(started, "benchmark requires nonempty stream");
        measured.ns = first.ns; measured.reads = first.reads; measured.bytes = first.bytes;
    }
    return measured;
}
int main(int argc, char** argv) {
    require(argc == 4, "PHASE ITERATIONS FILE");
    const std::string phase = argv[1];
    require(phase == "full" || phase == "stream4096" || phase == "startup", "unknown phase");
    size_t iterations = std::stoull(argv[2]); require(iterations != 0, "zero iterations");
    std::ifstream input(argv[3], std::ios::binary); require(input.good(), "open input");
    std::vector<char> bytes((std::istreambuf_iterator<char>(input)), {});
    run(bytes, phase); // Warm process/stream caches, matching the Rust worker.
    Measurement total;
    for (size_t i = 0; i < iterations; ++i) {
        auto measured = run(bytes, phase);
        total.ns += measured.ns; total.samples += measured.samples;
        total.reads += measured.reads; total.bytes += measured.bytes;
    }
    std::printf("libvorbis\t%s\t%zu\t%llu\t%llu\t%llu\t%llu\n", phase.c_str(), iterations,
        static_cast<unsigned long long>(total.ns), static_cast<unsigned long long>(total.samples),
        static_cast<unsigned long long>(total.reads), static_cast<unsigned long long>(total.bytes));
}

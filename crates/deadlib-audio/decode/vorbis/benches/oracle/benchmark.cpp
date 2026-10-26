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
extern "C" {
#include "mdct.h"
#include "codebook.h"
}

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
static uint64_t integer(const std::vector<char>& input, size_t& position, size_t bytes) {
    require(position + bytes <= input.size(), "truncated codeword corpus");
    uint64_t value = 0;
    for (size_t i = 0; i < bytes; ++i) value |= static_cast<uint64_t>(static_cast<unsigned char>(input[position++])) << (8 * i);
    return value;
}
static void huffman(const std::vector<char>& input, size_t iterations) {
    require(input.size() >= 12 && std::memcmp(input.data(), "DLVHUF1\0", 8) == 0, "codeword corpus signature");
    size_t position = 8;
    size_t count = static_cast<size_t>(integer(input, position, 4));
    require(count > 0 && count <= 256, "codeword book count");
    struct Book { codebook tree{}; std::vector<char> lengths, wire; std::vector<long> expected; };
    std::vector<Book> books(count);
    uint64_t symbols = 0, expected_sum = 0;
    for (auto& book : books) {
        size_t entries = static_cast<size_t>(integer(input, position, 4));
        require(position + entries <= input.size(), "codeword lengths");
        book.lengths.assign(input.begin() + position, input.begin() + position + entries); position += entries;
        uint64_t bits = integer(input, position, 8);
        size_t bytes = static_cast<size_t>(integer(input, position, 4));
        require(position + bytes <= input.size(), "codeword bytes");
        book.wire.assign(input.begin() + position, input.begin() + position + bytes); position += bytes;
        static_codebook config{}; config.dim = 1; config.entries = static_cast<long>(entries); config.lengthlist = book.lengths.data();
        require(vorbis_book_init_decode(&book.tree, &config) == 0, "codeword decode init");
        oggpack_buffer reader; oggpack_readinit(&reader, reinterpret_cast<unsigned char*>(book.wire.data()), static_cast<int>(book.wire.size()));
        for (size_t entry = 0; entry < entries; ++entry) if (book.lengths[entry] != 0) {
            require(vorbis_book_decode(&book.tree, &reader) == static_cast<long>(entry), "codeword warm parity");
            book.expected.push_back(static_cast<long>(entry)); expected_sum += entry;
        }
        require(static_cast<uint64_t>(oggpack_bits(&reader)) == bits, "codeword consumed bits");
        symbols += book.expected.size();
    }
    require(position == input.size(), "codeword corpus trailing bytes");
    uint64_t sum = 0;
    const auto begin = Clock::now();
    for (size_t i = 0; i < iterations; ++i) {
        for (auto& book : books) {
            oggpack_buffer reader; oggpack_readinit(&reader, reinterpret_cast<unsigned char*>(book.wire.data()), static_cast<int>(book.wire.size()));
            for (size_t j = 0; j < book.expected.size(); ++j) {
                long value = vorbis_book_decode(&book.tree, &reader); require(value >= 0, "codeword decode");
                sum += static_cast<uint64_t>(value);
            }
        }
    }
    const auto ns = std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now() - begin).count();
    require(sum == expected_sum * iterations, "codeword checksum");
    for (auto& book : books) vorbis_book_clear(&book.tree);
    std::printf("libvorbis\thuffman\t%zu\t%llu\t%llu\t0\t0\n", iterations, static_cast<unsigned long long>(ns), static_cast<unsigned long long>(symbols * iterations));
}
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
    require(argc == 4, "PHASE ITERATIONS FILE_OR_BLOCK_SIZE");
    const std::string phase = argv[1];
    size_t iterations = std::stoull(argv[2]); require(iterations != 0, "zero iterations");
    if (phase == "imdct") {
        size_t n = std::stoull(argv[3]);
        require(n >= 64 && n <= 8192 && (n & (n - 1)) == 0, "MDCT block size");
        mdct_lookup transform{}; mdct_init(&transform, static_cast<int>(n));
        std::vector<float> spectrum(n / 2), output(n);
        for (size_t i = 0; i < n / 2; ++i) spectrum[i] = (static_cast<int>(i * 17 % 101) - 50) / 256.0f;
        mdct_backward(&transform, spectrum.data(), output.data());
        const auto begin = Clock::now();
        for (size_t i = 0; i < iterations; ++i) {
            mdct_backward(&transform, spectrum.data(), output.data());
            sink += output.front() + output.back();
        }
        const auto ns = std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now() - begin).count();
        mdct_clear(&transform);
        std::printf("libvorbis\timdct\t%zu\t%llu\t%zu\t0\t0\n", iterations, static_cast<unsigned long long>(ns), iterations * n);
        return 0;
    }
    require(phase == "full" || phase == "stream4096" || phase == "startup" || phase == "huffman", "unknown phase");
    std::ifstream input(argv[3], std::ios::binary); require(input.good(), "open input");
    std::vector<char> bytes((std::istreambuf_iterator<char>(input)), {});
    if (phase == "huffman") { huffman(bytes, iterations); return 0; }
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

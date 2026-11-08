#ifndef DEADLIB_SEEK_BENCHMARK_SUPPORT_H
#define DEADLIB_SEEK_BENCHMARK_SUPPORT_H
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <vorbis/vorbisfile.h>
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <filesystem>
#include <limits>
#include <string>
#include <vector>
extern "C" {
// The pinned C internal floor structure has a field named `class`.
#define class vorbis_class
#include "codec_internal.h"
#undef class
}
#undef min
#undef max

namespace seek_bench {
inline void require(bool value, const char *reason) {
    if (!value) { std::fprintf(stderr, "%s\n", reason); std::exit(1); }
}
inline std::vector<char> load(const std::filesystem::path& path) {
    std::ifstream input(path, std::ios::binary); require(input.good(), "open benchmark input");
    return {(std::istreambuf_iterator<char>(input)), {}};
}
struct IoStats { uint64_t reads = 0, bytes = 0, seeks = 0, tells = 0; };
struct Source {
    const std::vector<char>& data;
    uint64_t position = 0;
    IoStats io;
    static size_t read(void *out, size_t size, size_t count, void *opaque) {
        auto& source = *static_cast<Source*>(opaque); ++source.io.reads;
        if (size == 0 || source.position >= source.data.size()) return 0;
        size_t items = std::min(count, static_cast<size_t>(source.data.size() - source.position) / size);
        size_t bytes = items * size;
        std::memcpy(out, source.data.data() + source.position, bytes);
        source.position += bytes; source.io.bytes += bytes; return items;
    }
    static int seek(void *opaque, ogg_int64_t offset, int whence) {
        auto& source = *static_cast<Source*>(opaque); ++source.io.seeks;
        uint64_t base;
        if (whence == SEEK_SET) base = 0;
        else if (whence == SEEK_CUR) base = source.position;
        else if (whence == SEEK_END) base = source.data.size();
        else return -1;
        if (offset < 0) {
            uint64_t magnitude = static_cast<uint64_t>(-(offset + 1)) + 1;
            if (magnitude > base) return -1;
            source.position = base - magnitude;
        } else {
            if (base > std::numeric_limits<uint64_t>::max() - static_cast<uint64_t>(offset)) return -1;
            source.position = base + static_cast<uint64_t>(offset);
        }
        return 0;
    }
    static long tell(void *opaque) {
        auto& source = *static_cast<Source*>(opaque); ++source.io.tells;
        if (source.position > static_cast<uint64_t>(std::numeric_limits<long>::max())) return -1;
        return static_cast<long>(source.position);
    }
};
struct Geometry { uint32_t channels, rate, large_block, floor0; uint64_t frames; };
struct Request { uint64_t target; uint32_t want; std::vector<std::vector<float>> planes; };
struct Workload {
    Geometry geometry{};
    std::vector<Request> requests;
    uint64_t expected_samples() const {
        uint64_t total = 0; for (const auto& request : requests) total += static_cast<uint64_t>(request.want) * geometry.channels; return total;
    }
    static Workload read(const std::filesystem::path& path, bool with_pcm) {
        std::ifstream file(path, std::ios::binary); require(file.good(), "open seek workload");
        auto integer = [&file](int bytes) {
            uint64_t value = 0;
            for (int i = 0; i < bytes; ++i) { int c = file.get(); require(c != EOF, "truncated seek workload"); value |= uint64_t(c) << (8 * i); }
            return value;
        };
        char signature[8]; file.read(signature, 8);
        require(file.gcount() == 8 && std::memcmp(signature, "DLVSEEK1", 8) == 0, "seek workload signature");
        Workload out;
        out.geometry.channels = static_cast<uint32_t>(integer(4)); out.geometry.rate = static_cast<uint32_t>(integer(4));
        out.geometry.large_block = static_cast<uint32_t>(integer(4)); out.geometry.floor0 = static_cast<uint32_t>(integer(4)); out.geometry.frames = integer(8);
        uint64_t count = integer(4);
        require(count > 0 && count <= 10000 && out.geometry.frames > 0 && out.geometry.channels > 0 && out.geometry.channels <= 255, "seek workload geometry");
        if (with_pcm) {
            for (uint64_t i = 0; i < count; ++i) {
                Request request; request.target = integer(8); request.want = static_cast<uint32_t>(integer(4));
                require(request.want > 0 && request.want <= 4096 && request.want <= out.geometry.frames && request.target <= out.geometry.frames - request.want, "seek workload interval");
                request.planes.resize(out.geometry.channels);
                for (auto& plane : request.planes) for (uint32_t j = 0; j < request.want; ++j) {
                    uint32_t raw = static_cast<uint32_t>(integer(4)); float value; std::memcpy(&value, &raw, 4);
                    require(std::isfinite(value), "nonfinite reference PCM"); plane.push_back(value);
                }
                out.requests.push_back(std::move(request));
            }
            require(file.get() == EOF, "seek workload trailing bytes");
        }
        return out;
    }
};
static volatile double sink = 0;
struct Reader {
    Source source;
    OggVorbis_File file{};
    explicit Reader(const std::vector<char>& data, bool seekable = true) : source{data} {
        ov_callbacks callbacks{Source::read, seekable ? Source::seek : nullptr, nullptr, seekable ? Source::tell : nullptr};
        require(ov_open_callbacks(&source, &file, nullptr, 0, callbacks) == 0, "reference seek open");
        require(ov_streams(&file) == 1, "benchmark requires single stream");
    }
    ~Reader() { ov_clear(&file); }
    Geometry geometry() {
        auto *info = ov_info(&file, -1); require(info != nullptr, "reference stream information");
        ogg_int64_t total = ov_pcm_total(&file, -1); require(total > 0, "nonempty seekable reference stream required");
        auto *setup = static_cast<codec_setup_info*>(info->codec_setup);
        uint32_t floor0 = 0; for (int i = 0; i < setup->floors; ++i) if (setup->floor_type[i] == 0) floor0 = 1;
        return {static_cast<uint32_t>(info->channels), static_cast<uint32_t>(info->rate), static_cast<uint32_t>(vorbis_info_blocksize(info, 1)), floor0, static_cast<uint64_t>(total)};
    }
    void check(Geometry expected) {
        auto actual = geometry();
        require(actual.frames == expected.frames && actual.channels == expected.channels && actual.rate == expected.rate && actual.large_block == expected.large_block, "reference geometry mismatch");
    }
    uint64_t decode(bool startup) {
        uint64_t samples = 0;
        for (;;) {
            int link; float **pcm; long frames = ov_read_float(&file, &pcm, 4096, &link);
            require(frames >= 0, "reference decode error"); if (frames == 0) break;
            require(link == 0, "single reference stream required");
            int channels = ov_info(&file, -1)->channels;
            for (int ch = 0; ch < channels; ++ch) sink += pcm[ch][0] + pcm[ch][frames - 1];
            samples += static_cast<uint64_t>(frames) * channels;
            if (startup) break;
        }
        return samples;
    }
    void warm(Geometry expected) {
        require(ov_pcm_seek(&file, 0) == 0, "reference warm rewind");
        require(decode(false) == expected.frames * expected.channels, "reference warm count");
    }
    uint64_t batch(const Workload& workload, bool validate) {
        uint64_t exposed = 0;
        for (const auto& request : workload.requests) {
            require(ov_pcm_seek(&file, static_cast<ogg_int64_t>(request.target)) == 0, "reference PCM seek");
            require(ov_pcm_tell(&file) == static_cast<ogg_int64_t>(request.target), "reference PCM seek position");
            uint32_t position = 0; double peak = 0, squared = 0, signal = 0; uint64_t count = 0;
            while (position < request.want) {
                int link; float **pcm;
                long frames = ov_read_float(&file, &pcm, static_cast<int>(request.want - position), &link);
                require(frames > 0 && link == 0, "reference seek EOF/error");
                for (uint32_t channel = 0; channel < workload.geometry.channels; ++channel) {
                    sink += pcm[channel][0] + pcm[channel][frames - 1];
                    if (validate) for (long i = 0; i < frames; ++i) {
                        float expected = request.planes[channel][position + i];
                        double error = std::abs(static_cast<double>(pcm[channel][i]) - expected);
                        require(std::isfinite(pcm[channel][i]), "nonfinite seek PCM");
                        peak = std::max(peak, error); squared += error * error; signal = std::max(signal, std::abs(static_cast<double>(expected))); ++count;
                    }
                }
                position += static_cast<uint32_t>(frames); exposed += frames * workload.geometry.channels;
            }
            require(position == request.want, "reference seek count");
            if (validate) {
                double scale = std::max(1.0, signal);
                require(count > 0 && peak <= (workload.geometry.floor0 ? 5e-5 : 3e-6) * scale && std::sqrt(squared / count) <= (workload.geometry.floor0 ? 3e-6 : 5e-7) * scale, "reference seek PCM mismatch");
            }
        }
        return exposed;
    }
};
}
#endif

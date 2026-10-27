// Test-only encoder/decoder worker using the pinned, unmodified Xiph sources.
#include <vorbis/vorbisenc.h>
#include <vorbis/vorbisfile.h>
#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iterator>
#include <string>
#include <tuple>
#include <vector>

using Clock = std::chrono::steady_clock;
static void require(bool ok, const char* message) {
    if (!ok) { std::fprintf(stderr, "%s\n", message); std::exit(1); }
}
static uint64_t ns(Clock::time_point start) {
    return static_cast<uint64_t>(std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now() - start).count());
}
static uint64_t integer(const std::vector<char>& bytes, size_t& offset, size_t n) {
    require(offset + n <= bytes.size(), "truncated PCM header"); uint64_t value = 0;
    for (size_t i = 0; i < n; ++i) value |= uint64_t(static_cast<unsigned char>(bytes[offset++])) << (8 * i);
    return value;
}
static void put(std::ofstream& file, uint64_t value, size_t bytes) {
    for (size_t i = 0; i < bytes; ++i) file.put(static_cast<char>(value >> (8 * i)));
}
struct Pcm { uint32_t channels, rate; size_t frames; std::vector<float> samples; };
static Pcm load(const char* path) {
    std::ifstream file(path, std::ios::binary); require(file.good(), "open PCM");
    std::vector<char> bytes((std::istreambuf_iterator<char>(file)), {});
    require(bytes.size() >= 24 && std::memcmp(bytes.data(), "DLVPCM1\0", 8) == 0, "PCM signature");
    size_t offset = 8; Pcm pcm{};
    pcm.channels = static_cast<uint32_t>(integer(bytes, offset, 4));
    pcm.rate = static_cast<uint32_t>(integer(bytes, offset, 4)); pcm.frames = static_cast<size_t>(integer(bytes, offset, 8));
    require(pcm.channels > 0 && pcm.channels <= 255 && pcm.rate > 0 && pcm.frames <= 100000000, "PCM geometry");
    require(bytes.size() == 24 + pcm.frames * pcm.channels * 4, "PCM length");
    pcm.samples.resize(pcm.frames * pcm.channels);
    for (auto& sample : pcm.samples) {
        uint32_t word = static_cast<uint32_t>(integer(bytes, offset, 4)); std::memcpy(&sample, &word, 4);
        require(std::isfinite(sample) && std::fabs(sample) <= 4.f, "PCM range");
    }
    return pcm;
}
static void save(const Pcm& pcm, const char* path) {
    std::ofstream file(path, std::ios::binary); require(file.good(), "create PCM");
    file.write("DLVPCM1\0", 8); put(file, pcm.channels, 4); put(file, pcm.rate, 4); put(file, pcm.frames, 8);
    for (auto sample : pcm.samples) { uint32_t word; std::memcpy(&word, &sample, 4); put(file, word, 4); }
    require(file.good(), "write PCM");
}
static volatile uint64_t sink = 0;
struct Output {
    uint64_t bytes = 0; bool retain; std::vector<char> output;
    void write(const unsigned char* data, size_t count) {
        if (count) sink = sink ^ data[0] ^ data[count - 1];
        bytes += count;
        if (retain) output.insert(output.end(), data, data + count);
    }
    void page(const ogg_page& page) {
        write(page.header, static_cast<size_t>(page.header_len));
        write(page.body, static_cast<size_t>(page.body_len));
    }
};
struct Measured {
    uint64_t full_ns = 0, open_ns = 0, first_ns = 0, bytes = 0, packets = 0;
    size_t first_input = 0, first_granule = 0, first_packet_bytes = 0;
    auto geometry() const { return std::make_tuple(bytes, packets, first_input, first_granule, first_packet_bytes); }
};
static Measured run(const Pcm& pcm, float quality, Output& output) {
    const auto begin = Clock::now();
    vorbis_info info; vorbis_comment comments; vorbis_dsp_state dsp; vorbis_block block; ogg_stream_state stream;
    vorbis_info_init(&info);
    require(vorbis_encode_init_vbr(&info, static_cast<long>(pcm.channels), static_cast<long>(pcm.rate), quality) == 0, "vorbis_encode_init_vbr");
    vorbis_comment_init(&comments); vorbis_comment_add_tag(&comments, "TITLE", "encoding benchmark");
    require(vorbis_analysis_init(&dsp, &info) == 0, "vorbis_analysis_init");
    require(vorbis_block_init(&dsp, &block) == 0, "vorbis_block_init");
    require(ogg_stream_init(&stream, 123) == 0, "ogg_stream_init");
    ogg_packet id{}, comment{}, setup{}; ogg_page page{};
    require(vorbis_analysis_headerout(&dsp, &comments, &id, &comment, &setup) == 0, "vorbis_analysis_headerout");
    for (auto* packet : {&id, &comment, &setup}) require(ogg_stream_packetin(&stream, packet) == 0, "header packetin");
    while (ogg_stream_flush(&stream, &page)) output.page(page);
    Measured measured; measured.open_ns = ns(begin); size_t supplied = 0; bool eos = false;
    for (;;) {
        size_t count = std::min(size_t(4096), pcm.frames - supplied);
        if (count) {
            float** buffer = vorbis_analysis_buffer(&dsp, static_cast<int>(count)); require(buffer != nullptr, "analysis buffer");
            for (size_t frame = 0; frame < count; ++frame) for (size_t ch = 0; ch < pcm.channels; ++ch)
                buffer[ch][frame] = pcm.samples[(supplied + frame) * pcm.channels + ch];
        }
        supplied += count;
        require(vorbis_analysis_wrote(&dsp, static_cast<int>(count)) == 0, "analysis wrote");
        int available;
        while ((available = vorbis_analysis_blockout(&dsp, &block)) == 1) {
            require(vorbis_analysis(&block, nullptr) == 0, "vorbis_analysis");
            require(vorbis_bitrate_addblock(&block) == 0, "bitrate_addblock");
            ogg_packet packet{}; int queued;
            while ((queued = vorbis_bitrate_flushpacket(&dsp, &packet)) == 1) {
                measured.packets++;
                require(ogg_stream_packetin(&stream, &packet) == 0, "audio packetin");
                while (ogg_stream_pageout(&stream, &page)) output.page(page);
                if (!measured.first_ns && packet.granulepos > 0) {
                    measured.first_ns = ns(begin); measured.first_input = supplied;
                    measured.first_granule = static_cast<size_t>(packet.granulepos);
                    measured.first_packet_bytes = static_cast<size_t>(packet.bytes);
                }
                eos |= packet.e_o_s != 0;
            }
            require(queued == 0, "bitrate_flushpacket");
        }
        require(available == 0, "analysis_blockout");
        if (!count) break;
    }
    require(eos && measured.first_ns > 0, "missing EOS or post-priming packet");
    while (ogg_stream_flush(&stream, &page)) output.page(page);
    measured.bytes = output.bytes;
    ogg_stream_clear(&stream); vorbis_block_clear(&block); vorbis_dsp_clear(&dsp);
    vorbis_comment_clear(&comments); vorbis_info_clear(&info);
    measured.full_ns = ns(begin); return measured;
}
static void encode(const Pcm& pcm, const char* quality_text, size_t iterations, const std::string& destination) {
    require(iterations > 0 && (destination == "-" || iterations == 1), "iterations/output");
    float quality = std::stof(quality_text); require(quality >= -.1f && quality <= 1.f, "quality range");
    Output discarded{0, false, {}}; const Measured warm = run(pcm, quality, discarded);
    Measured total;
    for (size_t i = 0; i < iterations; ++i) {
        Output output{0, destination != "-", {}}; const Measured value = run(pcm, quality, output);
        require(value.geometry() == warm.geometry(), "nondeterministic output geometry");
        total.full_ns += value.full_ns; total.open_ns += value.open_ns; total.first_ns += value.first_ns;
        if (output.retain) {
            std::ofstream file(destination, std::ios::binary); require(file.good(), "create Ogg");
            file.write(output.output.data(), static_cast<std::streamsize>(output.output.size())); require(file.good(), "write Ogg");
        }
    }
    std::printf("libvorbis\t%s\t%zu\t%llu\t%llu\t%llu\t%zu\t%llu\t%llu\t%zu\t%zu\t%zu\n",
        quality_text, iterations, static_cast<unsigned long long>(total.full_ns), static_cast<unsigned long long>(total.open_ns),
        static_cast<unsigned long long>(total.first_ns), pcm.frames, static_cast<unsigned long long>(warm.bytes),
        static_cast<unsigned long long>(warm.packets), warm.first_input, warm.first_granule, warm.first_packet_bytes);
}
static void decode(const char* source, const char* destination, int64_t start = -1, uint64_t count = 0) {
    FILE* input = std::fopen(source, "rb"); require(input != nullptr, "open Ogg"); OggVorbis_File reader{};
    require(ov_open_callbacks(input, &reader, nullptr, 0, OV_CALLBACKS_NOCLOSE) == 0, "ov_open_callbacks");
    require(ov_streams(&reader) == 1, "single-stream output");
    auto* info = ov_info(&reader, 0); require(info && info->channels > 0 && info->rate > 0, "Ogg geometry");
    ogg_int64_t total = ov_pcm_total(&reader, 0); require(total >= 0 && total <= 100000000, "Ogg total");
    if (start >= 0) require(start <= total && count <= static_cast<uint64_t>(total - start), "extract range");
    Pcm pcm{static_cast<uint32_t>(info->channels), static_cast<uint32_t>(info->rate), start >= 0 ? static_cast<size_t>(count) : static_cast<size_t>(total), {}};
    pcm.samples.reserve(pcm.frames * pcm.channels);
    if (pcm.frames) {
        require((start >= 0 ? ov_pcm_seek(&reader, start) : ov_raw_seek(&reader, 0)) == 0, "initial seek");
        if (start >= 0) require(ov_pcm_tell(&reader) == start, "extract seek position");
    }
    if (pcm.frames) for (;;) {
        const auto remaining = pcm.frames - pcm.samples.size() / pcm.channels;
        if (start >= 0 && !remaining) break;
        float** planes; int link; long count = ov_read_float(&reader, &planes, start >= 0 ? static_cast<int>(std::min(size_t(4096), remaining)) : 4096, &link);
        require(count >= 0, "ov_read_float"); if (!count) break; require(link == 0, "unexpected chained output");
        for (long frame = 0; frame < count; ++frame) for (size_t ch = 0; ch < pcm.channels; ++ch) pcm.samples.push_back(planes[ch][frame]);
    }
    ov_clear(&reader); std::fclose(input);
    require(pcm.samples.size() == pcm.frames * pcm.channels, "declared/emitted PCM mismatch"); save(pcm, destination);
}
int main(int argc, char** argv) {
    require(argc >= 2, "encode QUALITY ITERATIONS PCM OUTPUT_OR_DASH | decode OGG PCM | extract OGG PCM START_FRAMES FRAMES");
    if (std::string(argv[1]) == "encode") { require(argc == 6, "encode arguments"); encode(load(argv[4]), argv[2], std::stoull(argv[3]), argv[5]); }
    else if (std::string(argv[1]) == "decode") { require(argc == 4, "decode arguments"); decode(argv[2], argv[3]); }
    else if (std::string(argv[1]) == "extract") { require(argc == 6, "extract arguments"); decode(argv[2], argv[3], std::stoll(argv[4]), std::stoull(argv[5])); }
    else require(false, "unknown mode");
}

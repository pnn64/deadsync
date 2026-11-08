#include "seek_support.h"
#define DEADLIB_ALLOC_IMPLEMENTATION
#include "alloc_profile.h"
#include <chrono>
#include <memory>
#ifdef _WIN32
#define NOMINMAX
#include <windows.h>
#include <psapi.h>
#endif
using namespace seek_bench;
static void begin_profile() {
#ifdef DEADLIB_ALLOC_PROFILE
    dlab_begin();
#endif
}
static dlab_stats end_profile() {
#ifdef DEADLIB_ALLOC_PROFILE
    return dlab_end();
#else
    return {};
#endif
}
static std::string peak_working_set() {
#ifdef _WIN32
    PROCESS_MEMORY_COUNTERS_EX counters{}; counters.cb = sizeof(counters);
    require(GetProcessMemoryInfo(GetCurrentProcess(), reinterpret_cast<PROCESS_MEMORY_COUNTERS*>(&counters), sizeof(counters)) != 0, "query process memory");
    return std::to_string(counters.PeakWorkingSetSize);
#else
    return "null";
#endif
}
template<class Character> int entry(int argc, Character **argv) {
    require(argc >= 2, "PHASE FILE REQUESTS or --selftest");
    std::string phase;
    for (auto *c = argv[1]; *c; ++c) { require(*c < 128, "ASCII phase required"); phase.push_back(static_cast<char>(*c)); }
#ifdef DEADLIB_ALLOC_PROFILE
    constexpr bool counted = true;
#else
    constexpr bool counted = false;
#endif
    if (phase == "--selftest") {
#ifdef DEADLIB_ALLOC_PROFILE
        dlab_self_test(); std::puts("allocator-selftest:ok"); return 0;
#else
        require(false, "selftest requires allocation instrumentation");
#endif
    }
    require(argc == 4 || (argc == 5 && (phase == "seek_timing" || phase == "seek_open_timing")), "PHASE FILE REQUESTS [ITERATIONS]");
    auto data = load(std::filesystem::path(argv[2]));
    if (phase == "info") {
        Reader reader(data); auto geometry = reader.geometry();
        std::printf("%llu\t%u\t%u\t%u\t%u\n", static_cast<unsigned long long>(geometry.frames), geometry.channels, geometry.rate, geometry.large_block, geometry.floor0); return 0;
    }
    auto workload = Workload::read(std::filesystem::path(argv[3]), phase == "seek_batch" || phase == "seek_timing");
    if (phase == "seek_open_timing") {
        require(argc == 5, "seek_open_timing requires iteration count");
        size_t iterations = static_cast<size_t>(std::stoull(std::basic_string<Character>(argv[4])));
        require(iterations > 0, "positive iterations required");
        { Reader warm(data); warm.check(workload.geometry); }
        uint64_t elapsed = 0; IoStats io;
        for (size_t i = 0; i < iterations; ++i) {
            auto begin = std::chrono::steady_clock::now();
            { Reader reader(data); reader.check(workload.geometry);
              io.reads += reader.source.io.reads; io.bytes += reader.source.io.bytes;
              io.seeks += reader.source.io.seeks; io.tells += reader.source.io.tells; }
            elapsed += std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now() - begin).count();
        }
        std::printf("libvorbis\tseek_open\t%zu\t%llu\t%zu\t%llu\t%llu\t%llu\t%llu\t0\n", iterations,
            static_cast<unsigned long long>(elapsed), iterations, static_cast<unsigned long long>(io.reads), static_cast<unsigned long long>(io.bytes),
            static_cast<unsigned long long>(io.seeks), static_cast<unsigned long long>(io.tells));
        return 0;
    }
    if (phase == "seek_timing") {
        require(argc == 5, "seek_timing requires iteration count");
        std::basic_string<Character> argument = argv[4];
        size_t iterations = static_cast<size_t>(std::stoull(argument)); require(iterations > 0, "positive iterations required");
        Reader reader(data); reader.check(workload.geometry); reader.warm(workload.geometry); reader.batch(workload, true); reader.source.io = {};
        auto begin = std::chrono::steady_clock::now(); uint64_t exposed = 0;
        for (size_t i = 0; i < iterations; ++i) exposed += reader.batch(workload, false);
        auto ns = std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now() - begin).count();
        auto io = reader.source.io;
        std::printf("libvorbis\tseek_batch\t%zu\t%llu\t%llu\t%llu\t%llu\t%llu\t%llu\t%llu\n", iterations,
            static_cast<unsigned long long>(ns), static_cast<unsigned long long>(workload.expected_samples() * iterations),
            static_cast<unsigned long long>(io.reads), static_cast<unsigned long long>(io.bytes), static_cast<unsigned long long>(io.seeks), static_cast<unsigned long long>(io.tells), static_cast<unsigned long long>(exposed));
        return 0;
    }
    require(phase == "full" || phase == "startup" || phase == "seek_open" || phase == "seek_batch", "unknown resource phase");
    if (counted && phase != "seek_batch") {
        Reader warm(data); warm.warm(workload.geometry);
    }
    dlab_stats memory{}; IoStats io; uint64_t exposed = 0;
    if (phase == "seek_batch") {
        Reader reader(data); reader.check(workload.geometry); reader.warm(workload.geometry); reader.batch(workload, true); reader.source.io = {};
        begin_profile(); exposed = reader.batch(workload, false); memory = end_profile(); io = reader.source.io;
    } else if (phase == "seek_open") {
        begin_profile(); Reader reader(data); reader.check(workload.geometry); memory = end_profile(); io = reader.source.io;
    } else {
        begin_profile();
        {
            Reader reader(data, false); exposed = reader.decode(phase == "startup");
            if (phase == "startup") memory = end_profile();
            else require(exposed == workload.geometry.frames * workload.geometry.channels, "full reference count");
            io = reader.source.io;
        }
        if (phase == "full") { memory = end_profile(); if (counted) require(memory.retained_delta_bytes == 0, "full reference retained heap"); }
    }
    std::string operations = counted ? std::to_string(memory.operations) : "null";
    std::string requested = counted ? std::to_string(memory.requested_bytes) : "null";
    std::string peak = counted ? std::to_string(memory.peak_extra_bytes) : "null";
    std::string retained = counted ? std::to_string(memory.retained_delta_bytes) : "null";
    std::string working_set = counted ? "null" : peak_working_set();
    std::printf("{\"engine\":\"libvorbis\",\"phase\":\"%s\",\"instrumented\":%s,\"operations\":%s,\"requested_bytes\":%s,\"peak_extra_bytes\":%s,\"retained_delta_bytes\":%s,\"process_peak_working_set_bytes\":%s,\"reads\":%llu,\"bytes_read\":%llu,\"seeks\":%llu,\"tells\":%llu,\"api_exposed_samples\":%llu}\n",
        phase.c_str(), counted ? "true" : "false", operations.c_str(), requested.c_str(), peak.c_str(), retained.c_str(), working_set.c_str(),
        static_cast<unsigned long long>(io.reads), static_cast<unsigned long long>(io.bytes), static_cast<unsigned long long>(io.seeks), static_cast<unsigned long long>(io.tells), static_cast<unsigned long long>(exposed));
    return 0;
}
#ifdef _WIN32
int wmain(int argc, wchar_t **argv) { return entry(argc, argv); }
#else
int main(int argc, char **argv) { return entry(argc, argv); }
#endif

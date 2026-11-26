cmake_minimum_required(VERSION 3.24)
project(jpeg_contenders LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded")
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY "${WORK}/bridges")
set(UPSTREAM "${WORK}/upstream")
set(COMMON_FLAGS _CRT_SECURE_NO_WARNINGS)

add_library(bench_turbo SHARED libjpeg.c)
target_include_directories(bench_turbo PRIVATE "${UPSTREAM}/turbo/src" "${WORK}/turbo-build")
target_compile_definitions(bench_turbo PRIVATE HIGH_PRECISION ${COMMON_FLAGS})
target_link_libraries(bench_turbo PRIVATE "${WORK}/turbo-build/Release/jpeg-static.lib")

add_library(bench_mozjpeg SHARED libjpeg.c)
target_include_directories(bench_mozjpeg PRIVATE "${UPSTREAM}/mozjpeg" "${WORK}/moz-static")
target_compile_definitions(bench_mozjpeg PRIVATE HIGH_PRECISION MOZ_JPEG ${COMMON_FLAGS})
target_link_libraries(bench_mozjpeg PRIVATE "${WORK}/moz-static/Release/jpeg-static.lib")

add_library(bench_jpegli SHARED libjpegli.cpp)
set_property(TARGET bench_jpegli PROPERTY MSVC_RUNTIME_LIBRARY "MultiThreadedDLL")
target_include_directories(bench_jpegli PRIVATE "${WORK}/jpegli-build/lib/include/jpegli" "${UPSTREAM}/jpegli")
target_compile_definitions(bench_jpegli PRIVATE JPEG_LI ${COMMON_FLAGS})
target_link_libraries(bench_jpegli PRIVATE "${WORK}/jpegli-build/lib/Release/jpegli-static.lib"
    "${WORK}/jpegli-build/third_party/highway/Release/hwy.lib")

set(SJPEG_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SJPEG_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(SJPEG_ENABLE_MULTITHREADING OFF CACHE BOOL "" FORCE)
set(SJPEG_HAVE_AVX2 ON CACHE BOOL "" FORCE)
add_subdirectory("${UPSTREAM}/sjpeg" sjpeg EXCLUDE_FROM_ALL)
add_library(bench_extra SHARED extra.cpp "${UPSTREAM}/jpgd/jpgd.cpp")
target_compile_options(bench_extra PRIVATE /arch:AVX2)
target_include_directories(bench_extra PRIVATE "${UPSTREAM}/sjpeg/src" "${UPSTREAM}/jpgd"
    "${WUFFS}/release/c")
target_compile_definitions(bench_extra PRIVATE ${COMMON_FLAGS})
target_link_libraries(bench_extra PRIVATE sjpeg)

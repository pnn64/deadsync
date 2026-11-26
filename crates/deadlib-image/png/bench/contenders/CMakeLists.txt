cmake_minimum_required(VERSION 3.24)
project(png_contenders LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(PNG_SHARED OFF CACHE BOOL "" FORCE)
set(PNG_STATIC ON CACHE BOOL "" FORCE)
set(PNG_TESTS OFF CACHE BOOL "" FORCE)
set(PNG_TOOLS OFF CACHE BOOL "" FORCE)
set(PNG_LIBCONF_HEADER "${UPSTREAM}/libpng/pnglibconf.h.prebuilt" CACHE FILEPATH "")
set(ZLIB_USE_STATIC_LIBS ON)
find_package(ZLIB REQUIRED)
add_subdirectory("${UPSTREAM}/libpng" libpng EXCLUDE_FROM_ALL)

add_library(png_contenders SHARED contenders.c fast.cpp
    "${UPSTREAM}/libspng/spng/spng.c" "${UPSTREAM}/fpng/src/fpng.cpp"
    "${UPSTREAM}/fpnge/fpnge.cc")
target_compile_definitions(png_contenders PRIVATE SPNG_STATIC _CRT_SECURE_NO_WARNINGS)
target_include_directories(png_contenders PRIVATE
    "${UPSTREAM}/libpng" "${CMAKE_CURRENT_BINARY_DIR}/libpng"
    "${UPSTREAM}/libspng/spng" "${UPSTREAM}/wuffs/release/c"
    "${UPSTREAM}/stb" "${UPSTREAM}/fpng/src" "${UPSTREAM}/fpnge")
target_link_libraries(png_contenders PRIVATE png_static ZLIB::ZLIB)
if(MSVC)
    target_compile_options(png_contenders PRIVATE /arch:AVX2)
else()
    set_source_files_properties("${UPSTREAM}/fpnge/fpnge.cc" PROPERTIES COMPILE_OPTIONS "-mavx2;-mpclmul;-mbmi2")
endif()

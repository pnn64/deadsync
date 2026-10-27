#ifndef DEADLIB_ALLOCATION_PROFILE_H
#define DEADLIB_ALLOCATION_PROFILE_H
#include <stdlib.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
typedef struct {
    uint64_t operations, requested_bytes, peak_extra_bytes;
    int64_t retained_delta_bytes;
} dlab_stats;
void *dlab_malloc(size_t bytes);
void *dlab_calloc(size_t count, size_t bytes);
void *dlab_realloc(void *pointer, size_t bytes);
void dlab_free(void *pointer);
void dlab_begin(void);
dlab_stats dlab_end(void);
void dlab_self_test(void);
#ifdef __cplusplus
}
#endif
#ifndef DEADLIB_ALLOC_IMPLEMENTATION
#define malloc dlab_malloc
#define calloc dlab_calloc
#define realloc dlab_realloc
#define free dlab_free
#endif
#endif

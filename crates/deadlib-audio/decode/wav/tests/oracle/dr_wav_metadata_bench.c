#define DR_WAV_IMPLEMENTATION
#include "dr_wav.h"
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <assert.h>
#ifdef _WIN32
#include <windows.h>
static double now(void) {LARGE_INTEGER n,f;QueryPerformanceCounter(&n);QueryPerformanceFrequency(&f);return (double)n.QuadPart/(double)f.QuadPart;}
static uint64_t cycles(void) {ULONG64 c=0;assert(QueryThreadCycleTime(GetCurrentThread(),&c));return (uint64_t)c;}
#else
#include <time.h>
static double now(void) {struct timespec n;clock_gettime(CLOCK_MONOTONIC,&n);return (double)n.tv_sec+1e-9*n.tv_nsec;}
static uint64_t cycles(void) {return 0;}
#endif
/* The 16-byte header retains malloc alignment; its cost is excluded from the
 * requested-byte counter, just as the Rust counter excludes allocator overhead. */
typedef struct {size_t size;uint64_t padding;} Header;
static size_t calls,requested,live,peak;
static int tracking=0;
static void allocated(size_t n) {if(tracking){++calls;requested+=n;live+=n;if(live>peak)peak=live;}}
static void *on_malloc(size_t n,void *user) {Header *p;(void)user;p=(Header*)malloc(sizeof(Header)+n);if(!p)return NULL;p->size=n;allocated(n);return p+1;}
static void on_free(void *p,void *user) {Header *h;(void)user;if(!p)return;h=((Header*)p)-1;if(tracking)live-=h->size;free(h);}
static void *on_realloc(void *p,size_t n,void *user) {Header *h,*next;size_t old;(void)user;if(!p)return on_malloc(n,user);h=((Header*)p)-1;old=h->size;next=(Header*)realloc(h,sizeof(Header)+n);if(!next)return NULL;next->size=n;if(tracking)live-=old;allocated(n);return next+1;}
static drwav_allocation_callbacks allocator={NULL,on_malloc,on_realloc,on_free};
static uint64_t checksum(const drwav *wav) {
    uint64_t sum=0;drwav_uint32 i,j,k;
    for(i=0;i<wav->metadataCount;++i) {
        const drwav_metadata *m=&wav->pMetadata[i];
        if(m->type==drwav_metadata_type_cue) {
            for(j=0;j<m->data.cue.cuePointCount;++j) {
                const drwav_cue_point *p=&m->data.cue.pCuePoints[j];
                sum+=(uint64_t)p->id+p->playOrderPosition+p->chunkStart+p->blockStart+p->sampleOffset;
                for(k=0;k<4;++k)sum+=p->dataChunkId[k];
            }
        } else if(m->type==drwav_metadata_type_list_info_title) {
            for(j=0;j<m->data.infoText.stringLength;++j)sum+=(unsigned char)m->data.infoText.pString[j];
        }
    }
    return sum;
}
typedef struct {const void *bytes;size_t size;drwav *retained;uint64_t expected;int mode;} Work;
static volatile uint64_t sink;
static void operation(const Work *w) {
    drwav wav;uint64_t actual;
    if(w->mode==2){actual=checksum(w->retained);assert(actual==w->expected);sink=actual;return;}
    if(w->mode==0){assert(drwav_init_memory(&wav,w->bytes,w->size,&allocator));sink=wav.totalPCMFrameCount;}
    else {assert(drwav_init_memory_with_metadata(&wav,w->bytes,w->size,0,&allocator));actual=checksum(&wav);assert(actual==w->expected);sink=actual;}
    drwav_uninit(&wav);
}
static void bench(const char *name,size_t iters,Work *w) {
    int trial;size_t i,allocs,req,pk;double start,elapsed;uint64_t cpu;
    operation(w);calls=requested=live=peak=0;tracking=1;operation(w);tracking=0;
    assert(live==0);allocs=calls;req=requested;pk=peak;
    for(trial=0;trial<5;++trial){start=now();cpu=cycles();for(i=0;i<iters;++i)operation(w);cpu=cycles()-cpu;elapsed=now()-start;
        printf("%s\t%d\t%zu\t%.3f\t%.3f\t%zu\t%zu\t%zu\n",name,trial,iters,elapsed*1e9/iters,(double)cpu/iters,allocs,req,pk);}
}
int main(int argc,char **argv) {
    FILE *f;long size;unsigned char *bytes;drwav retained;size_t iters;Work w;
    if(argc!=4)return 2;
    f=fopen(argv[1],"rb");if(!f)return 3;fseek(f,0,SEEK_END);size=ftell(f);rewind(f);if(size<0)return 4;
    bytes=(unsigned char*)malloc((size_t)size);if(!bytes)return 5;if(fread(bytes,1,(size_t)size,f)!=(size_t)size)return 6;fclose(f);
    iters=(size_t)strtoull(argv[2],NULL,10);w.bytes=bytes;w.size=(size_t)size;w.expected=(uint64_t)strtoull(argv[3],NULL,10);w.retained=&retained;w.mode=0;
    assert(drwav_init_memory_with_metadata(&retained,bytes,(size_t)size,0,&allocator));assert(checksum(&retained)==w.expected);
    printf("operation\ttrial\titers\tns_per_op\tthread_cycles_per_op\talloc_calls\trequested_bytes\tpeak_requested_live_bytes\n");
    bench("dr_wav/open_headers",iters,&w);w.mode=1;bench("dr_wav/open_walk_metadata",iters,&w);w.mode=2;bench("dr_wav/walk_retained_views",iters,&w);
    drwav_uninit(&retained);free(bytes);return 0;
}

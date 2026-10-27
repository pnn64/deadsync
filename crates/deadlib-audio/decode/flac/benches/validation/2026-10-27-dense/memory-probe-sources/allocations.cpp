// Logical requested-heap accounting for a separate, single-thread decoder probe.
// These wrappers preserve allocator pointers and also accept untracked frees.
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <pthread.h>
#include <unistd.h>

extern "C" {
void* __real_malloc(size_t);
void* __real_calloc(size_t,size_t);
void* __real_realloc(void*,size_t);
void __real_free(void*);
int __real_posix_memalign(void**,size_t,size_t);
void* __real_aligned_alloc(size_t,size_t);
}
namespace {
constexpr size_t capacity=1<<18;
struct Entry { uintptr_t address; size_t bytes; };
struct Stats { size_t calls=0,reallocs=0,frees=0,requested=0,live=0,peak=0; };
Entry entries[capacity];
Stats stats;
std::atomic<bool> active{false};
pthread_mutex_t mutex=PTHREAD_MUTEX_INITIALIZER;
constexpr uintptr_t tombstone=1;
size_t hash(uintptr_t address){return ((address>>4)*UINT64_C(11400714819323198485))&(capacity-1);}
void insert(void* pointer,size_t bytes){
    uintptr_t address=reinterpret_cast<uintptr_t>(pointer);
    size_t i=hash(address);
    for(size_t n=0;n<capacity;n++,i=(i+1)&(capacity-1)){
        if(!entries[i].address||entries[i].address==tombstone){entries[i]={address,bytes};return;}
    }
    _exit(90);
}
bool remove(uintptr_t address){
    if(!address)return false;
    size_t i=hash(address);
    for(size_t n=0;n<capacity;n++,i=(i+1)&(capacity-1)){
        if(!entries[i].address)return false;
        if(entries[i].address==address){stats.live-=entries[i].bytes;entries[i]={tombstone,0};return true;}
    }
    return false;
}
void allocation(void* pointer,size_t bytes){
    if(!pointer||!active.load(std::memory_order_relaxed))return;
    pthread_mutex_lock(&mutex);
    if(active.load(std::memory_order_relaxed)){
        insert(pointer,bytes);stats.calls++;stats.requested+=bytes;stats.live+=bytes;
        if(stats.live>stats.peak)stats.peak=stats.live;
    }
    pthread_mutex_unlock(&mutex);
}
void release(void* pointer){
    if(!pointer||!active.load(std::memory_order_relaxed))return;
    pthread_mutex_lock(&mutex);
    if(active.load(std::memory_order_relaxed)&&remove(reinterpret_cast<uintptr_t>(pointer)))stats.frees++;
    pthread_mutex_unlock(&mutex);
}
void reallocation(uintptr_t old_address,void* pointer,size_t bytes){
    if((!pointer&&bytes)||!active.load(std::memory_order_relaxed))return;
    pthread_mutex_lock(&mutex);
    if(active.load(std::memory_order_relaxed)){
        remove(old_address);stats.calls++;stats.reallocs++;stats.requested+=bytes;
        if(pointer){insert(pointer,bytes);stats.live+=bytes;if(stats.live>stats.peak)stats.peak=stats.live;}
    }
    pthread_mutex_unlock(&mutex);
}
}
extern "C" void* __wrap_malloc(size_t n){auto* p=__real_malloc(n);allocation(p,n);return p;}
extern "C" void* __wrap_calloc(size_t n,size_t size){auto* p=__real_calloc(n,size);if(p)allocation(p,n*size);return p;}
extern "C" void* __wrap_realloc(void* p,size_t n){auto address=reinterpret_cast<uintptr_t>(p);auto* q=__real_realloc(p,n);reallocation(address,q,n);return q;}
extern "C" void __wrap_free(void* p){release(p);__real_free(p);}
extern "C" int __wrap_posix_memalign(void** p,size_t alignment,size_t n){int status=__real_posix_memalign(p,alignment,n);if(!status)allocation(*p,n);return status;}
extern "C" void* __wrap_aligned_alloc(size_t alignment,size_t n){auto* p=__real_aligned_alloc(alignment,n);allocation(p,n);return p;}
extern "C" char* __wrap_strdup(const char* text){size_t n=std::strlen(text)+1;auto* p=static_cast<char*>(__real_malloc(n));if(p){std::memcpy(p,text,n);allocation(p,n);}return p;}
extern "C" char* __wrap_strndup(const char* text,size_t limit){size_t n=strnlen(text,limit);auto* p=static_cast<char*>(__real_malloc(n+1));if(p){std::memcpy(p,text,n);p[n]=0;allocation(p,n+1);}return p;}
extern "C" void deadlib_alloc_begin(){
    pthread_mutex_lock(&mutex);
    active.store(false,std::memory_order_relaxed);
    std::memset(entries,0,sizeof(entries));stats={};
    active.store(true,std::memory_order_relaxed);
    pthread_mutex_unlock(&mutex);
}
extern "C" void deadlib_alloc_end(size_t iteration){
    pthread_mutex_lock(&mutex);
    active.store(false,std::memory_order_relaxed);Stats result=stats;
    pthread_mutex_unlock(&mutex);
    std::fprintf(stderr,"ALLOC\t%zu\t%zu\t%zu\t%zu\t%zu\t%zu\t%zu\t0\n",iteration,result.calls,result.reallocs,result.frees,result.requested,result.peak,result.live);
}

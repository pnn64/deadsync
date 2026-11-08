/* Lua execution stays in Rust. Only this C boundary owns setjmp/longjmp. */
#define BOUNDARY_BUILD
#include "api.h"
#include <setjmp.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
static SRWLOCK state_lock=SRWLOCK_INIT;
#define LOCK_STATES() AcquireSRWLockExclusive(&state_lock)
#define UNLOCK_STATES() ReleaseSRWLockExclusive(&state_lock)
#else
#include <stdatomic.h>
static atomic_flag state_lock=ATOMIC_FLAG_INIT;
#define LOCK_STATES() while(atomic_flag_test_and_set_explicit(&state_lock,memory_order_acquire)){}
#define UNLOCK_STATES() atomic_flag_clear_explicit(&state_lock,memory_order_release)
#endif

typedef struct BoundaryGlobal {
  size_t references;
  lua_CFunction panic;
  void *owner;
  BoundaryDispatch dispatch;
  BoundaryClose close;
} BoundaryGlobal;
typedef struct BoundaryState {
  lua_State *identity;
  BoundaryGlobal *global;
  struct BoundaryState *next;
  int owns_global;
} BoundaryState;
static BoundaryState *states;
#ifdef DEADLUA_EMBED_RUNTIME
extern lua_State *deadlua_engine_new_state(const void *);
extern unsigned long long deadlua_engine_abi_version(void);
static BoundaryCreate create_state=(BoundaryCreate)deadlua_engine_new_state;
#else
static BoundaryCreate create_state;
#endif
static BoundaryState *find_state(lua_State *L) {
  BoundaryState *state;
  for(state=states;state;state=state->next)if(state->identity==L)return state;
  return NULL;
}
static int default_panic(lua_State *L) {
  fprintf(stderr,"PANIC: unprotected error in call to Lua API (%s)\n",lua_tolstring(L,-1,NULL));
  return 0;
}
API lua_CFunction lua_atpanic(lua_State *L,lua_CFunction callback) {
  lua_CFunction previous;BoundaryState *state;
  LOCK_STATES();state=find_state(L);
  if(!state){UNLOCK_STATES();abort();}
  previous=state->global->panic;state->global->panic=callback;
  UNLOCK_STATES();return previous;
}
static lua_CFunction current_panic(lua_State *L) {
  lua_CFunction callback;BoundaryState *state;
  LOCK_STATES();state=find_state(L);
  if(!state){UNLOCK_STATES();abort();}
  callback=state->global->panic;UNLOCK_STATES();
  return callback;
}
static void operation(lua_State *L,int op,int a,int b,BoundaryValue *value);
static void panic_error(lua_State *L,lua_CFunction callback) {
  BoundaryValue value={0};
  if(callback)callback(L);
  /* C exit flushes stdio. Flush the Rust-owned streams too, after a callback
     has had its opportunity to recover without terminating the process. */
  operation(L,74,0,0,&value);
  exit(EXIT_FAILURE);
}

typedef struct BoundaryFrame {
  void *context;
  BoundaryDispatch dispatch;
  jmp_buf failure;
  struct BoundaryFrame *previous;
  lua_State *previous_active;
  lua_State *state;
} BoundaryFrame;
struct lua_State {
  size_t opaque_identity;
};
#ifdef _WIN32
#define THREAD_LOCAL __declspec(thread)
#else
#define THREAD_LOCAL _Thread_local
#endif
static THREAD_LOCAL lua_State *active_state;
static THREAD_LOCAL BoundaryFrame *active_frame;
#ifndef DEADLUA_ABI_VERSION
#define DEADLUA_ABI_VERSION 0
#endif
API unsigned long long boundary_abi_version(void) {
#ifdef DEADLUA_EMBED_RUNTIME
  return deadlua_engine_abi_version();
#else
  return DEADLUA_ABI_VERSION;
#endif
}
API void boundary_engine_create(BoundaryCreate create) {
  LOCK_STATES();create_state=create;UNLOCK_STATES();
}
static void engine_anchor(void) {}
API lua_State *luaL_newstate(void) {
  BoundaryCreate create;
  LOCK_STATES();create=create_state;UNLOCK_STATES();
  return create?create((void *)engine_anchor):NULL;
}
API int boundary_state_own(lua_State *L,void *owner,BoundaryDispatch dispatch,BoundaryClose close) {
  BoundaryState *state;int result=0;
  LOCK_STATES();state=find_state(L);
  if(state && !state->global->owner) {
    state->global->owner=owner;state->global->dispatch=dispatch;state->global->close=close;
    result=1;
  }
  UNLOCK_STATES();return result;
}
API void lua_close(lua_State *L) {
  BoundaryState *state;BoundaryFrame *frame;BoundaryGlobal *global;
  void *owner;BoundaryClose close;
  LOCK_STATES();state=find_state(L);
  if(!state || !state->global->owner){UNLOCK_STATES();abort();}
  global=state->global;
  for(frame=active_frame;frame;frame=frame->previous) {
    BoundaryState *active=find_state(frame->state);
    if(active && active->global==global){UNLOCK_STATES();abort();}
  }
  owner=global->owner;close=global->close;
  UNLOCK_STATES();close(owner);
}

API int boundary_read(lua_State *L, void *context, BoundaryDispatch dispatch, lua_Reader reader, void *data, BoundaryValue *result) {
  BoundaryFrame frame;
  frame.context=context;frame.dispatch=dispatch;frame.previous=active_frame;frame.previous_active=active_state;frame.state=L;
  active_frame=&frame;active_state=L;
  if (setjmp(frame.failure)) {
    active_frame=frame.previous;active_state=frame.previous_active;return -1;
  }
  result->length=0;
  result->bytes=reader(L,data,&result->length);
  active_frame=frame.previous;active_state=frame.previous_active;
  return 0;
}
API int boundary_write(lua_State *L, void *context, BoundaryDispatch dispatch, lua_Writer writer, void *data, const void *bytes, size_t length, BoundaryValue *result) {
  BoundaryFrame frame;
  frame.context=context;frame.dispatch=dispatch;frame.previous=active_frame;frame.previous_active=active_state;frame.state=L;
  active_frame=&frame;active_state=L;
  if (setjmp(frame.failure)) {
    active_frame=frame.previous;active_state=frame.previous_active;return -1;
  }
  result->integer=writer(L,bytes,length,data);
  active_frame=frame.previous;active_state=frame.previous_active;return 0;
}
API int boundary_hook(lua_State *L, void *context, BoundaryDispatch dispatch, lua_Hook hook, lua_Debug *ar) {
  BoundaryFrame frame;
  frame.context=context;frame.dispatch=dispatch;frame.previous=active_frame;frame.previous_active=active_state;frame.state=L;
  active_frame=&frame;active_state=L;
  if (setjmp(frame.failure)) {
    active_frame=frame.previous;active_state=frame.previous_active;return -1;
  }
  hook(L,ar);
  active_frame=frame.previous;active_state=frame.previous_active;return 0;
}

static void operation(lua_State *L, int op, int a, int b, BoundaryValue *value) {
  /* dispatch has returned, releasing every Rust borrow/local, before jumping. */
  BoundaryFrame *frame = active_frame;
  BoundaryDispatch dispatch=frame?frame->dispatch:NULL;
  void *context=frame?frame->context:NULL;
  int root=0;
  lua_CFunction panic=op==7?current_panic(L):NULL;
  if(!frame || frame->state!=L) {
    BoundaryState *target,*active;
    LOCK_STATES();target=find_state(L);active=frame?find_state(frame->state):NULL;
    if(target && target->global->owner && (!active || target->global!=active->global)) {
      dispatch=target->global->dispatch;context=target->global->owner;root=1;
    }
    UNLOCK_STATES();
  }
  if (!dispatch) abort();
  value->state=L;
  if(op==7)value->integer=panic!=NULL;
  {
    int result=dispatch(context, op, a, b, value);
    if(result==3) {
      /* Finalizers may replace the callback while an API operation runs. */
      panic=current_panic(L);value->integer=panic!=NULL;
      result=dispatch(context,72,0,0,value);
    }
    if(result==2 || (root && result))panic_error(L,current_panic(L));
    if(result)longjmp(frame->failure, 1);
  }
}
static void check_gc(lua_State *L) {
  BoundaryValue value={0};operation(L,73,0,0,&value);
}
#include "builtin-bodies.h"
API lua_CFunction lua_tocfunction(lua_State *L,int index) {
  BoundaryValue value={0};operation(L,70,index,0,&value);
  if(value.userdata)return (lua_CFunction)value.userdata;
  return value.integer>=0 && value.integer<(int)(sizeof(builtin_bodies)/sizeof(builtin_bodies[0])) ? builtin_bodies[value.integer] : NULL;
}
static void library_hook(lua_State *L, lua_Debug *ar) {
  static const char *const names[]={"call","return","line","count","tail return"};
  BoundaryValue value={0};operation(L,69,0,0,&value);
  if(value.integer) {
    int line=ar->currentline;
    lua_pushstring(L,names[ar->event]);
    if(line>=0)lua_pushinteger(L,line);else lua_pushnil(L);
    lua_getinfo(L,"lS",ar);lua_call(L,2,0);
  }
}
API int lua_sethook(lua_State *L, lua_Hook hook, int mask, int count) {
  BoundaryValue value={0};value.userdata=(void *)hook;value.integer=hook==library_hook;
  operation(L,65,mask,count,&value);return 1;
}
API lua_Hook lua_gethook(lua_State *L) {
  BoundaryValue value={0};operation(L,66,0,0,&value);
  return value.integer?library_hook:(lua_Hook)value.userdata;
}
API int lua_gethookmask(lua_State *L) {
  BoundaryValue value={0};operation(L,67,0,0,&value);return value.integer;
}
API int lua_gethookcount(lua_State *L) {
  BoundaryValue value={0};operation(L,68,0,0,&value);return value.integer;
}
API int lua_load(lua_State *L, lua_Reader reader, void *data, const char *name) {
  BoundaryValue value={0};value.bytes=name;value.reader=reader;value.userdata=data;
  operation(L,48,0,0,&value);return value.integer;
}
API int lua_dump(lua_State *L, lua_Writer writer, void *data) {
  BoundaryValue value={0};value.writer=writer;value.userdata=data;
  operation(L,50,0,0,&value);return value.integer;
}
API lua_State *lua_tothread(lua_State *L, int index) {
  BoundaryValue value={0};operation(L,51,index,0,&value);return (lua_State *)value.userdata;
}
API lua_State *lua_newthread(lua_State *L) {
  BoundaryValue value={0};operation(L,55,0,0,&value);return (lua_State *)value.userdata;
}
API void lua_xmove(lua_State *from, lua_State *to, int count) {
  BoundaryValue value={0};value.userdata=to;operation(from,56,count,0,&value);
}
API int lua_resume(lua_State *L, int arguments) {
  BoundaryValue value={0};operation(L,57,arguments,0,&value);return value.integer;
}
API int lua_yield(lua_State *L, int results) {
  BoundaryValue value={0};operation(L,58,results,0,&value);return -1;
}
API void lua_setlevel(lua_State *from, lua_State *to) {
  BoundaryValue value={0};value.userdata=to;operation(from,59,0,0,&value);
}
API int boundary_call_level(lua_State *L) {
  BoundaryValue value={0};operation(L,60,0,0,&value);return value.integer;
}
API int lua_getstack(lua_State *L, int level, lua_Debug *ar) {
  BoundaryValue value={0};value.userdata=ar;operation(L,61,level,0,&value);return value.integer;
}
API int lua_getinfo(lua_State *L, const char *options, lua_Debug *ar) {
  BoundaryValue value={0};value.userdata=ar;value.bytes=options;operation(L,62,0,0,&value);return value.integer;
}
API const char *lua_getlocal(lua_State *L, const lua_Debug *ar, int n) {
  BoundaryValue value={0};value.userdata=(void *)ar;operation(L,63,n,0,&value);return value.bytes;
}
API const char *lua_setlocal(lua_State *L, const lua_Debug *ar, int n) {
  BoundaryValue value={0};value.userdata=(void *)ar;operation(L,64,n,0,&value);return value.bytes;
}
API int lua_pushthread(lua_State *L) {
  BoundaryValue value={0};operation(L,52,0,0,&value);return value.integer;
}
API int lua_status(lua_State *L) {
  BoundaryValue value={0};value.userdata=L;
  operation(L,53,0,0,&value);return value.integer;
}
API const void *lua_topointer(lua_State *L, int index) {
  BoundaryValue value={0};operation(L,54,index,0,&value);return value.userdata;
}
typedef struct BufferReader {const char *bytes;size_t size;} BufferReader;
static const char *read_buffer(lua_State *L, void *data, size_t *size) {
  BufferReader *input=(BufferReader *)data;
  const char *result=input->bytes;(void)L;
  *size=input->size;input->size=0;return result;
}
API int luaL_loadbuffer(lua_State *L, const char *bytes, size_t length, const char *name) {
  BufferReader input;input.bytes=bytes;input.size=length;
  return lua_load(L,read_buffer,&input,name);
}
API int luaL_loadstring(lua_State *L, const char *string) {
  return luaL_loadbuffer(L,string,strlen(string),string);
}
API int luaL_loadfile(lua_State *L, const char *name) {
  BoundaryValue value={0};value.bytes=name;operation(L,49,0,0,&value);return value.integer;
}

API double lua_tonumber(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 35, index, 0, &value); return value.number;
}
API int lua_isnumber(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 35, index, 0, &value); return value.integer;
}
API lua_Integer lua_tointeger(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 36, index, 0, &value); return value.wide_integer;
}
API int lua_toboolean(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 37, index, 0, &value); return value.integer;
}
API void lua_pushinteger(lua_State *L, lua_Integer integer) {
  BoundaryValue value = {0}; value.wide_integer = integer; operation(L, 38, 0, 0, &value);
}
API int lua_isstring(lua_State *L, int index) {
  int type = lua_type(L, index); return type == 3 || type == 4;
}
API int lua_isuserdata(lua_State *L, int index) {
  int type = lua_type(L, index); return type == 2 || type == 7;
}
API int lua_iscfunction(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 41, index, 0, &value); return value.integer;
}
API void lua_pushstring(lua_State *L, const char *string) {
  if (string) lua_pushlstring(L, string, strlen(string)); else lua_pushnil(L);
}
API const char *lua_typename(lua_State *L, int type) {
  static const char *const names[] = {"no value", "nil", "boolean", "userdata", "number", "string", "table", "function", "userdata", "thread"};
  (void)L; return type >= -1 && type <= 8 ? names[type+1] : NULL;
}
API int lua_equal(lua_State *L, int left, int right) {
  BoundaryValue value = {0}; operation(L, 39, left, right, &value); return value.integer;
}
API int lua_lessthan(lua_State *L, int left, int right) {
  BoundaryValue value = {0}; value.integer = 1; operation(L, 39, left, right, &value); return value.integer;
}
API void lua_concat(lua_State *L, int count) {
  if(count>=2)check_gc(L);
  BoundaryValue value = {0}; operation(L, 40, count, 0, &value);
}
API const char *lua_getupvalue(lua_State *L, int index, int number) {
  BoundaryValue value = {0}; operation(L, 45, index, number, &value); return value.bytes;
}
API const char *lua_setupvalue(lua_State *L, int index, int number) {
  BoundaryValue value = {0}; operation(L, 46, index, number, &value); return value.bytes;
}
API int lua_gc(lua_State *L, int what, int data) {
  BoundaryValue value = {0}; operation(L, 47, what, data, &value); return value.integer;
}

API void *lua_newuserdata(lua_State *L, size_t size) {
  check_gc(L);
  BoundaryValue value = {0}; value.length = size;
  operation(L, 29, 0, 0, &value);
  return lua_touserdata(L, -1);
}
API size_t lua_objlen(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 30, index, 0, &value);
  return value.length;
}
API const char *lua_tolstring(lua_State *L, int index, size_t *length) {
  int converted=lua_type(L,index)==3;
  BoundaryValue value = {0}; operation(L, 31, index, 0, &value);
  if(converted) {
    check_gc(L);
    /* Collection may replace a slot or move a borrowed thread window. */
    operation(L,31,index,0,&value);
  }
  if (length) *length = value.length;
  return value.bytes;
}
API void lua_getfenv(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 32, index, 0, &value);
}
API int lua_setfenv(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 33, index, 0, &value);
  return value.integer;
}
API int lua_pcall(lua_State *L, int arguments, int results, int handler) {
  BoundaryValue value = {0}; value.integer = handler;
  operation(L, 34, arguments, results, &value);
  return value.integer;
}
API int lua_cpcall(lua_State *L, lua_CFunction function, void *data) {
  lua_pushcclosure(L, function, 0);
  lua_pushlightuserdata(L, data);
  return lua_pcall(L, 1, 0, 0);
}

API lua_State *boundary_state_new(void *identity,lua_State *parent) {
  BoundaryState *state,*from;BoundaryGlobal *global;
  LOCK_STATES();from=parent?find_state(parent):NULL;
  if(parent && !from){UNLOCK_STATES();return NULL;}
  global=from?from->global:(BoundaryGlobal *)malloc(sizeof(BoundaryGlobal));
  if(!global){UNLOCK_STATES();return NULL;}
  state=(BoundaryState *)malloc(sizeof(BoundaryState));
  if(!state){if(!from)free(global);UNLOCK_STATES();return NULL;}
  if(!from){memset(global,0,sizeof(*global));global->panic=default_panic;}
  global->references++;state->identity=(lua_State *)identity;state->global=global;
  state->owns_global=!from;state->next=states;states=state;UNLOCK_STATES();
  /* Preserve the VM's aligned opaque thread identity without dereferencing it. */
  return (lua_State *)identity;
}
API size_t boundary_state_size(lua_State *L) {
  BoundaryState *state;size_t size=0;
  LOCK_STATES();state=find_state(L);
  if(state)size=sizeof(BoundaryState)+(state->owns_global?sizeof(BoundaryGlobal):0);
  UNLOCK_STATES();return size;
}
API int boundary_state_free(lua_State *state) {
  BoundaryFrame *frame;
  if (!state) return 0;
  for(frame=active_frame;frame;frame=frame->previous)if(frame->state==state)return 0;
  {
    BoundaryState **link,*removed;int last;
    LOCK_STATES();for(link=&states;*link && (*link)->identity!=state;link=&(*link)->next){}
    removed=*link;if(!removed){UNLOCK_STATES();return 0;}
    *link=removed->next;last=--removed->global->references==0;
    if(last)free(removed->global);
    free(removed);UNLOCK_STATES();return 1+last;
  }
}
API int boundary_invoke(lua_State *state, void *context, BoundaryDispatch dispatch, lua_CFunction f) {
  BoundaryFrame frame;
  int count;
  frame.context = context;
  frame.dispatch = dispatch;
  frame.previous = active_frame;
  frame.previous_active = active_state;
  frame.state=state;
  active_frame = &frame;
  active_state = state;
  if (setjmp(frame.failure)) {
    active_frame = frame.previous;
    active_state = frame.previous_active;
    return -1;
  }
  count = f(state);
  active_frame = frame.previous;
  active_state = frame.previous_active;
  return count;
}

API int lua_gettop(lua_State *L) {
  BoundaryValue value = {0};
  operation(L, 0, 0, 0, &value);
  return value.integer;
}
API void lua_pushnumber(lua_State *L, double number) {
  BoundaryValue value = {0}; value.number = number;
  operation(L, 1, 0, 0, &value);
}
API void lua_pushboolean(lua_State *L, int boolean) {
  BoundaryValue value = {0}; value.integer = boolean;
  operation(L, 2, 0, 0, &value);
}
API void lua_pushnil(lua_State *L) {
  BoundaryValue value = {0}; operation(L, 3, 0, 0, &value);
}
static void push_raw_bytes(lua_State *L, const char *bytes, size_t length) {
  BoundaryValue value = {0}; value.bytes = bytes; value.length = length;
  operation(L, 4, 0, 0, &value);
}
static void push_raw_string(lua_State *L,const char *text) {
  if(text)push_raw_bytes(L,text,strlen(text));else lua_pushnil(L);
}
API void lua_pushlstring(lua_State *L, const char *bytes, size_t length) {
  check_gc(L);push_raw_bytes(L,bytes,length);
}
API void lua_pushvalue(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 5, index, 0, &value);
}
API double luaL_checknumber(lua_State *L, int index) {
  double number = lua_tonumber(L, index);
  if (number == 0 && !lua_isnumber(L, index)) luaL_typerror(L, index, "number");
  return number;
}
API int lua_error(lua_State *L) {
  BoundaryValue value = {0}; operation(L, 7, 0, 0, &value);
  return 0;
}
API void lua_call(lua_State *L, int arguments, int results) {
  BoundaryValue value = {0}; operation(L, 8, arguments, results, &value);
}
API int lua_checkstack(lua_State *L, int extra) {
  BoundaryValue value = {0}; operation(L, 9, extra, 0, &value);
  return value.integer;
}
API void lua_settop(lua_State *L, int top) {
  BoundaryValue value = {0}; operation(L, 10, top, 0, &value);
}
API void lua_replace(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 11, index, 0, &value);
}
API void lua_pushlightuserdata(lua_State *L, void *pointer) {
  BoundaryValue value = {0}; value.bytes = (const char *)pointer;
  operation(L, 12, 0, 0, &value);
}
API void *lua_touserdata(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 13, index, 0, &value);
  return (void *)value.bytes;
}
API void lua_pushcclosure(lua_State *L, lua_CFunction function, int count) {
  check_gc(L);
  /* The fixture targets the pinned MSVC ABI, where these pointer sizes match. */
  BoundaryValue value = {0}; value.bytes = (const char *)function;
  operation(L, 14, count, 0, &value);
}
API void lua_createtable(lua_State *L, int array, int records) {
  check_gc(L);
  BoundaryValue value = {0}; operation(L, 15, array, records, &value);
}
API void lua_gettable(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 16, index, 0, &value);
}
API void lua_rawget(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 16, index, 1, &value);
}
API void lua_settable(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 17, index, 0, &value);
}
API void lua_rawset(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 17, index, 1, &value);
}
API void lua_getfield(lua_State *L, int index, const char *name) {
  BoundaryValue value = {0}; value.bytes = name; value.length = strlen(name);
  operation(L, 18, index, 0, &value);
}
API void lua_setfield(lua_State *L, int index, const char *name) {
  BoundaryValue value = {0}; value.bytes = name; value.length = strlen(name);
  operation(L, 19, index, 0, &value);
}
API void lua_rawgeti(lua_State *L, int index, int key) {
  BoundaryValue value = {0}; operation(L, 20, index, key, &value);
}
API void lua_rawseti(lua_State *L, int index, int key) {
  BoundaryValue value = {0}; operation(L, 21, index, key, &value);
}
API int lua_getmetatable(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 22, index, 0, &value); return value.integer;
}
API int lua_setmetatable(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 23, index, 0, &value); return 1;
}
API int lua_next(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 24, index, 0, &value); return value.integer;
}
API int lua_type(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 25, index, 0, &value); return value.integer;
}
API int lua_rawequal(lua_State *L, int first, int second) {
  BoundaryValue value = {0}; operation(L, 26, first, second, &value); return value.integer;
}
API void lua_remove(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 27, index, 0, &value);
}
API void lua_insert(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 28, index, 0, &value);
}


/* C varargs stay in the ABI shim. String/number/table execution uses the VM. */
API const char *lua_pushvfstring(lua_State *L, const char *format, va_list arguments) {
  const char *at = format, *percent;
  int parts = 1;
  check_gc(L);push_raw_string(L, "");
  while ((percent = strchr(at, '%')) != NULL) {
    char one[3] = {0};
    push_raw_bytes(L, at, (size_t)(percent-at));
    switch (percent[1]) {
      case 's': { const char *text = va_arg(arguments, const char *);
        push_raw_string(L, text ? text : "(null)"); break; }
      case 'd': lua_pushnumber(L, va_arg(arguments, int)); break;
      case 'f': lua_pushnumber(L, va_arg(arguments, double)); break;
      case 'c': one[0] = (char)va_arg(arguments, int); push_raw_string(L, one); break;
      case 'p': { char pointer[4*sizeof(void *)+8];
        sprintf(pointer, "%p", va_arg(arguments, void *)); push_raw_string(L, pointer); break; }
      case '%': push_raw_string(L, "%"); break;
      default: one[0] = '%'; one[1] = percent[1]; push_raw_string(L, one); break;
    }
    parts += 2;
    if (!percent[1]) { at = percent+1; break; }
    at = percent+2;
  }
  push_raw_string(L, at);
  { BoundaryValue value = {0}; operation(L, 44, parts+1, 0, &value); }
  return lua_tolstring(L, -1, NULL);
}
API const char *lua_pushfstring(lua_State *L, const char *format, ...) {
  const char *text; va_list arguments;
  va_start(arguments, format); text = lua_pushvfstring(L, format, arguments); va_end(arguments);
  return text;
}
API void luaL_where(lua_State *L, int level) {
  BoundaryValue value = {0}; operation(L, 43, level, 0, &value);
}
API int luaL_error(lua_State *L, const char *format, ...) {
  va_list arguments;
  luaL_where(L, 1); va_start(arguments, format);
  lua_pushvfstring(L, format, arguments); va_end(arguments);
  lua_concat(L, 2); return lua_error(L);
}
API int luaL_argerror(lua_State *L, int index, const char *detail) {
  BoundaryValue value = {0};
  if (!detail) detail = "(null)";
  value.bytes = detail; value.length = strlen(detail);
  operation(L, 42, index, 0, &value);
  luaL_where(L, 1); lua_insert(L, -2); lua_concat(L, 2);
  return lua_error(L);
}
API int luaL_typerror(lua_State *L, int index, const char *expected) {
  const char *detail = lua_pushfstring(L, "%s expected, got %s", expected, lua_typename(L, lua_type(L, index)));
  return luaL_argerror(L, index, detail);
}
API void luaL_checktype(lua_State *L, int index, int type) {
  if (lua_type(L, index) != type) luaL_typerror(L, index, lua_typename(L, type));
}
API void luaL_checkany(lua_State *L, int index) {
  if (lua_type(L, index) == -1) luaL_argerror(L, index, "value expected");
}
API const char *luaL_checklstring(lua_State *L, int index, size_t *length) {
  const char *text = lua_tolstring(L, index, length);
  if (!text) luaL_typerror(L, index, "string");
  return text;
}
API const char *luaL_optlstring(lua_State *L, int index, const char *fallback, size_t *length) {
  if (lua_type(L, index) <= 0) {
    if (length) *length = fallback ? strlen(fallback) : 0;
    return fallback;
  }
  return luaL_checklstring(L, index, length);
}
API double luaL_optnumber(lua_State *L, int index, double fallback) {
  return lua_type(L, index) <= 0 ? fallback : luaL_checknumber(L, index);
}
API lua_Integer luaL_checkinteger(lua_State *L, int index) {
  lua_Integer integer = lua_tointeger(L, index);
  if (!integer && !lua_isnumber(L, index)) luaL_typerror(L, index, "number");
  return integer;
}
API lua_Integer luaL_optinteger(lua_State *L, int index, lua_Integer fallback) {
  return lua_type(L, index) <= 0 ? fallback : luaL_checkinteger(L, index);
}
API void luaL_checkstack(lua_State *L, int extra, const char *detail) {
  if (!lua_checkstack(L, extra)) luaL_error(L, "stack overflow (%s)", detail);
}
API int luaL_checkoption(lua_State *L, int index, const char *fallback, const char *const options[]) {
  const char *text = fallback ? luaL_optlstring(L,index,fallback,NULL) : luaL_checklstring(L,index,NULL);
  int i;
  for (i=0; options[i]; i++) if (!strcmp(text, options[i])) return i;
  return luaL_argerror(L,index,lua_pushfstring(L,"invalid option '%s'",text));
}
API int luaL_newmetatable(lua_State *L, const char *name) {
  lua_getfield(L,LUA_REGISTRYINDEX,name);
  if (lua_type(L,-1) != 0) return 0;
  lua_settop(L,-2); lua_newtable(L); lua_pushvalue(L,-1);
  lua_setfield(L,LUA_REGISTRYINDEX,name); return 1;
}
API void *luaL_checkudata(lua_State *L, int index, const char *name) {
  void *pointer = lua_touserdata(L,index);
  if (pointer && lua_getmetatable(L,index)) {
    lua_getfield(L,LUA_REGISTRYINDEX,name);
    if (lua_rawequal(L,-1,-2)) { lua_settop(L,-3); return pointer; }
  }
  luaL_typerror(L,index,name); return NULL;
}
API int luaL_getmetafield(lua_State *L, int index, const char *event) {
  if (!lua_getmetatable(L,index)) return 0;
  lua_pushstring(L,event); lua_rawget(L,-2);
  if (lua_type(L,-1) == 0) { lua_settop(L,-3); return 0; }
  lua_remove(L,-2); return 1;
}
static int absolute_index(lua_State *L, int index) {
  return index > 0 || index <= LUA_REGISTRYINDEX ? index : lua_gettop(L)+index+1;
}
API int luaL_callmeta(lua_State *L, int index, const char *event) {
  index = absolute_index(L,index);
  if (!luaL_getmetafield(L,index,event)) return 0;
  lua_pushvalue(L,index); lua_call(L,1,1); return 1;
}
API const char *luaL_findtable(lua_State *L, int index, const char *path, int hint) {
  const char *part = path;
  lua_pushvalue(L,index);
  for (;;) {
    const char *end = strchr(part,'.');
    if (!end) end = part+strlen(part);
    lua_pushlstring(L,part,(size_t)(end-part)); lua_rawget(L,-2);
    if (lua_type(L,-1) == 0) {
      lua_settop(L,-2); lua_createtable(L,0,*end == '.' ? 1 : hint);
      lua_pushlstring(L,part,(size_t)(end-part)); lua_pushvalue(L,-2); lua_settable(L,-4);
    } else if (lua_type(L,-1) != 5) {
      lua_settop(L,-3); return part;
    }
    lua_remove(L,-2);
    if (!*end) return NULL;
    part = end+1;
  }
}
API void luaL_openlib(lua_State *L, const char *name, const luaL_Reg *functions, int captures) {
  const luaL_Reg *entry;
  if (name) {
    int fields = 0;
    for (entry=functions; entry->name; entry++) fields++;
    luaL_findtable(L,LUA_REGISTRYINDEX,"_LOADED",1);
    lua_getfield(L,-1,name);
    if (lua_type(L,-1) != 5) {
      lua_settop(L,-2);
      if (luaL_findtable(L,LUA_GLOBALSINDEX,name,fields)) luaL_error(L,"name conflict for module '%s'",name);
      lua_pushvalue(L,-1); lua_setfield(L,-3,name);
    }
    lua_remove(L,-2); lua_insert(L,-(captures+1));
  }
  for (entry=functions; entry->name; entry++) {
    int i;
    for (i=0; i<captures; i++) lua_pushvalue(L,-captures);
    lua_pushcclosure(L,entry->func,captures); lua_setfield(L,-(captures+2),entry->name);
  }
  lua_settop(L,-captures-1);
}
API void luaL_register(lua_State *L, const char *name, const luaL_Reg *functions) {
  luaL_openlib(L,name,functions,0);
}
API int luaL_ref(lua_State *L, int index) {
  int reference;
  index = absolute_index(L,index);
  if (lua_type(L,-1) == 0) { lua_settop(L,-2); return -1; }
  lua_rawgeti(L,index,0); reference = (int)lua_tointeger(L,-1); lua_settop(L,-2);
  if (reference) { lua_rawgeti(L,index,reference); lua_rawseti(L,index,0); }
  else reference = (int)lua_objlen(L,index)+1;
  lua_rawseti(L,index,reference); return reference;
}
API void luaL_unref(lua_State *L, int index, int reference) {
  if (reference < 0) return;
  index = absolute_index(L,index);
  lua_rawgeti(L,index,0); lua_rawseti(L,index,reference);
  lua_pushinteger(L,reference); lua_rawseti(L,index,0);
}

/* Only strings emitted by this buffer occupy its live stack suffix. */
static int buffer_emit(luaL_Buffer *buffer) {
  size_t used = (size_t)(buffer->p - buffer->buffer);
  if (!used) return 0;
  lua_pushlstring(buffer->L, buffer->buffer, used);
  buffer->p = buffer->buffer;
  buffer->lvl++;
  return 1;
}
static void buffer_merge(luaL_Buffer *buffer) {
  int count = 1;
  size_t total;
  if (buffer->lvl < 2) return;
  total = lua_objlen(buffer->L, -1);
  while (count < buffer->lvl) {
    size_t next = lua_objlen(buffer->L, -count-1);
    if (buffer->lvl-count+1 < 10 && total <= next) break;
    total += next;
    count++;
  }
  lua_concat(buffer->L, count);
  buffer->lvl -= count-1;
}
API void luaL_buffinit(lua_State *L, luaL_Buffer *buffer) {
  buffer->L = L;
  buffer->p = buffer->buffer;
  buffer->lvl = 0;
}
API char *luaL_prepbuffer(luaL_Buffer *buffer) {
  if (buffer_emit(buffer)) buffer_merge(buffer);
  return buffer->buffer;
}
API void luaL_addlstring(luaL_Buffer *buffer, const char *bytes, size_t length) {
  while (length) {
    size_t room = (size_t)(buffer->buffer+LUAL_BUFFERSIZE-buffer->p);
    size_t part;
    if (!room) { luaL_prepbuffer(buffer); room = LUAL_BUFFERSIZE; }
    part = length < room ? length : room;
    memcpy(buffer->p, bytes, part);
    buffer->p += part;
    bytes += part;
    length -= part;
  }
}
API void luaL_addstring(luaL_Buffer *buffer, const char *string) {
  luaL_addlstring(buffer, string, strlen(string));
}
API void luaL_addvalue(luaL_Buffer *buffer) {
  size_t length;
  const char *bytes = lua_tolstring(buffer->L, -1, &length);
  size_t room = (size_t)(buffer->buffer+LUAL_BUFFERSIZE-buffer->p);
  if (length <= room) {
    /* Nonstrings have a null pointer and zero length in Lua 5.1. */
    if (length) memcpy(buffer->p, bytes, length);
    buffer->p += length;
    lua_settop(buffer->L, -2);
  } else {
    if (buffer_emit(buffer)) lua_insert(buffer->L, -2);
    buffer->lvl++;
    buffer_merge(buffer);
  }
}
API void luaL_pushresult(luaL_Buffer *buffer) {
  buffer_emit(buffer);
  lua_concat(buffer->L, buffer->lvl);
  buffer->lvl = 1;
}
API const char *luaL_gsub(lua_State *L, const char *string, const char *pattern, const char *replacement) {
  luaL_Buffer buffer;
  const char *match;
  size_t width = strlen(pattern);
  luaL_buffinit(L, &buffer);
  while ((match = strstr(string, pattern)) != NULL) {
    luaL_addlstring(&buffer, string, (size_t)(match-string));
    luaL_addstring(&buffer, replacement);
    string = match+width;
  }
  luaL_addstring(&buffer, string);
  luaL_pushresult(&buffer);
  return lua_tolstring(L, -1, NULL);
}
#include "standard-libraries.h"
#include "native-stdio.h"

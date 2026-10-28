/* Public Lua 5.1 panic registration and C-local recovery. */
#include <setjmp.h>
#include <stdlib.h>
#include <stdio.h>
#ifdef _WIN32
#define PANIC_TLS __declspec(thread)
#else
#define PANIC_TLS _Thread_local
#endif
typedef struct PanicTrap {
  jmp_buf escape;
  struct PanicTrap *previous;
  lua_State *caller,*target;
  int callback,status,top,type,same,identity,caller_status,has_frame;
} PanicTrap;
static PANIC_TLS PanicTrap *panic_trap;
static PANIC_TLS lua_CFunction original_panic;
static int panic_hook_a(lua_State *L);
static int panic_hook_b(lua_State *L);
static int panic_returning(lua_State *L) {
  printf("panic-return\t%d\t%d\n",lua_status(L),lua_gettop(L));return 0;
}
static int panic_code(lua_CFunction f) {
  return !f?0:f==panic_hook_a?1:f==panic_hook_b?2:f==panic_returning?3:4;
}
static int observe_panic(lua_State *L,int which) {
  PanicTrap *trap=panic_trap;lua_Debug ar;
  if(!trap)return panic_returning(L);
  if(!lua_checkstack(L,4))abort();
  trap->callback=which;trap->status=lua_status(L);trap->top=lua_gettop(L);
  trap->type=lua_type(L,1);trap->has_frame=lua_getstack(L,0,&ar);
  lua_pushthread(L);trap->identity=lua_tothread(L,-1)==trap->target;lua_settop(L,-2);
  lua_pushvalue(L,1);lua_xmove(L,trap->caller,1);
  trap->same=lua_rawequal(trap->caller,2,-1);lua_settop(trap->caller,-2);
  trap->caller_status=lua_status(trap->caller);
  if(which==2){lua_atpanic(L,panic_hook_a);lua_pushinteger(L,73);}
  longjmp(trap->escape,1);
  return 0;
}
static int panic_hook_a(lua_State *L) {return observe_panic(L,1);}
static int panic_hook_b(lua_State *L) {return observe_panic(L,2);}
static int panic_swap(lua_State *L) {
  lua_State *co=lua_tothread(L,1);lua_CFunction callback,old;
  int mode=(int)luaL_checkinteger(L,2);if(!co)co=L;
  callback=mode==1?panic_hook_a:mode==2?panic_hook_b:mode==3?panic_returning:mode==4?original_panic:NULL;
  old=lua_atpanic(co,callback);
  if(panic_code(old)==4)original_panic=old;
  if(mode==-1)lua_atpanic(co,old);
  lua_pushinteger(L,panic_code(old));return 1;
}
static int panic_raise(lua_State *L) {
  lua_State *co=lua_tothread(L,1);PanicTrap *trap;
  if(!co || co==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(L,16)||!lua_checkstack(co,2))return luaL_error(L,"panic fixture stack reserve");
  trap=(PanicTrap *)calloc(1,sizeof(PanicTrap));if(!trap)return luaL_error(L,"panic fixture allocation");
  trap->caller=L;trap->target=co;trap->previous=panic_trap;panic_trap=trap;
  if(!setjmp(trap->escape)) {
    lua_pushvalue(L,2);lua_xmove(L,co,1);lua_error(co);
    abort();
  }
  panic_trap=trap->previous;
  lua_pushinteger(L,trap->callback);lua_pushinteger(L,trap->status);lua_pushinteger(L,trap->top);
  lua_pushinteger(L,trap->type);lua_pushboolean(L,trap->same);lua_pushboolean(L,trap->identity);
  lua_pushinteger(L,trap->caller_status);lua_pushinteger(L,trap->has_frame);
  lua_pushinteger(L,lua_gettop(co));lua_pushinteger(L,lua_status(co));
  free(trap);return 10;
}
static int panic_unhandled(lua_State *L) {
  lua_State *co=lua_tothread(L,1);
  if(!co || co==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(co,1))return luaL_error(L,"panic fixture stack reserve");
  lua_pushvalue(L,2);lua_xmove(L,co,1);return lua_error(co);
}
static int panic_protected_error(lua_State *L) {lua_pushvalue(L,1);return lua_error(L);}

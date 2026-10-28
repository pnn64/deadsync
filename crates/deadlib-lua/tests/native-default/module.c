/* A normal module compiled with the official public Lua 5.1 headers. */
#include "lua.h"
#include "lauxlib.h"
#ifdef _WIN32
#define EXPORT __declspec(dllexport)
#else
#define EXPORT __attribute__((visibility("default")))
#endif
static int sum(lua_State *L) {lua_pushnumber(L,luaL_checknumber(L,1)+luaL_checknumber(L,2));return 1;}
static int count(lua_State *L) {
  lua_Integer n=lua_tointeger(L,lua_upvalueindex(1))+1;
  lua_pushinteger(L,n);lua_replace(L,lua_upvalueindex(1));lua_pushinteger(L,n);return 1;
}
static int counter(lua_State *L) {lua_pushinteger(L,luaL_checkinteger(L,1));lua_pushcclosure(L,count,1);return 1;}
static int state(lua_State *L) {lua_pushlightuserdata(L,L);lua_pushboolean(L,lua_pushthread(L));lua_remove(L,-2);return 2;}
static int fail(lua_State *L) {lua_settop(L,1);return lua_error(L);}
static int invoke(lua_State *L) {int n=lua_gettop(L)-1;lua_call(L,n,LUA_MULTRET);return lua_gettop(L);}
static int yield(lua_State *L) {return lua_yield(L,lua_gettop(L));}
static int body(lua_State *L) {
  lua_CFunction f=lua_tocfunction(L,1);if(!f)return luaL_error(L,"C function expected");lua_remove(L,1);return f(L);
}
static int finalize(lua_State *L) {
  lua_Integer *p=(lua_Integer *)luaL_checkudata(L,1,"native.default.userdata");
  if(*p){lua_getfield(L,LUA_REGISTRYINDEX,"native.default.finalized");lua_pushinteger(L,lua_tointeger(L,-1)+1);lua_setfield(L,LUA_REGISTRYINDEX,"native.default.finalized");*p=0;}
  return 0;
}
static int box(lua_State *L) {
  lua_Integer *p=(lua_Integer *)lua_newuserdata(L,sizeof(lua_Integer));*p=17;
  if(luaL_newmetatable(L,"native.default.userdata")){lua_pushcfunction(L,finalize);lua_setfield(L,-2,"__gc");}
  lua_setmetatable(L,-2);return 1;
}
#include "inactive.c"
#include "thread-calls.c"
#include "thread-loads.c"
#include "panic.c"
#include "thread-files.c"
#include "allocation-gc.c"
#include "thread-execution.c"
#include "thread-gc.c"
#include "waiting-errors.c"
#include "thread-unprotected.c"
#include "call-yields.c"
#include "thread-tables.c"
#include "table-yields.c"
static const luaL_Reg functions[]={{"sum",sum},{"counter",counter},{"state",state},{"fail",fail},{"invoke",invoke},{"yield",yield},{"body",body},{"box",box},{"inactive",inactive_objects},{"pcall_thread",inactive_pcall},{"cpcall_thread",inactive_cpcall},{"yield_tail",inactive_yield_tail},{"called",inactive_called},{NULL,NULL}};
EXPORT int luaopen_native(lua_State *L) {
  lua_newtable(L);luaL_register(L,NULL,functions);lua_pushcfunction(L,inactive_load);lua_setfield(L,-2,"load_thread");lua_pushvalue(L,LUA_ENVIRONINDEX);lua_setfield(L,-2,"environment");
  lua_pushcfunction(L,panic_swap);lua_setfield(L,-2,"panic_swap");lua_pushcfunction(L,panic_raise);lua_setfield(L,-2,"panic_raise");
  lua_pushcfunction(L,panic_unhandled);lua_setfield(L,-2,"panic_unhandled");lua_pushcfunction(L,panic_protected_error);lua_setfield(L,-2,"panic_protected_error");
  lua_pushcfunction(L,inactive_loadfile);lua_setfield(L,-2,"file_thread");
  lua_pushcfunction(L,inactive_filepanic);lua_setfield(L,-2,"file_panic");
  lua_pushcfunction(L,allocation_gc);lua_setfield(L,-2,"allocation_gc");
  lua_pushcfunction(L,inactive_construct);lua_setfield(L,-2,"construct_thread");
  lua_pushcfunction(L,inactive_format);lua_setfield(L,-2,"format_thread");
  lua_pushcfunction(L,inactive_concat);lua_setfield(L,-2,"concat_thread");
  lua_pushcfunction(L,inactive_concat_panic);lua_setfield(L,-2,"concat_thread_panic");
  lua_pushcfunction(L,inactive_stack_offsets);lua_setfield(L,-2,"stack_offsets");
  lua_pushcfunction(L,inactive_gc);lua_setfield(L,-2,"gc_thread");
  lua_pushcfunction(L,inactive_gc_panic);lua_setfield(L,-2,"gc_thread_panic");
  lua_pushcfunction(L,waiting_raise);lua_setfield(L,-2,"waiting_raise");
  lua_pushcfunction(L,waiting_snapshot);lua_setfield(L,-2,"waiting_snapshot");
  lua_pushcfunction(L,waiting_c_resume);lua_setfield(L,-2,"waiting_c_resume");
  lua_pushcfunction(L,inactive_call);lua_setfield(L,-2,"call_thread");
  lua_pushcfunction(L,inactive_call_panic);lua_setfield(L,-2,"call_thread_panic");
  lua_pushcfunction(L,selected_yield_call);lua_setfield(L,-2,"yield_call");
  lua_pushcfunction(L,selected_yield_resume);lua_setfield(L,-2,"yield_resume");
  lua_pushcfunction(L,selected_table);lua_setfield(L,-2,"table_thread");
  lua_pushcfunction(L,selected_table_panic);lua_setfield(L,-2,"table_thread_panic");
  lua_pushcfunction(L,selected_table_yield);lua_setfield(L,-2,"table_yield");
  lua_pushcfunction(L,selected_table_yield_all);lua_setfield(L,-2,"table_c_yield_all");
  lua_pushcfunction(L,selected_table_yield_tail);lua_setfield(L,-2,"table_c_yield_tail");
  lua_pushcfunction(L,selected_table_yield_zero);lua_setfield(L,-2,"table_c_yield_zero");
  lua_pushvalue(L,1);lua_setfield(L,-2,"name");return 1;
}
EXPORT int luaopen_native_root_leaf(lua_State *L) {return luaopen_native(L);}

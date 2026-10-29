/* The program owns every lua_State. No Rust VM/driver activation surrounds
 * either phase, and the states survive the first program's return. */
#include "native-root.c"
#include <setjmp.h>
static lua_State *owned_main,*owned_peer;
static jmp_buf owned_recovery;
static int close_hooks,close_finalizers;
static void closing_hook(lua_State *L,lua_Debug *ar) {(void)L;(void)ar;close_hooks++;}
static int closing_finalizer(lua_State *L) {
  int id=lua_tointeger(L,lua_upvalueindex(1));
  lua_Debug ar;
  close_finalizers++;
  number("close.finalizer",id);
  number("close.hook.installed",lua_gethook(L)!=NULL);
  number("close.parent",lua_getstack(L,1,&ar));
  if(id==3) {lua_pushliteral(L,"ignored close error");return lua_error(L);}
  return 0;
}
static void add_closing_userdata(lua_State *L,int id) {
  lua_newuserdata(L,1);lua_newtable(L);
  lua_pushinteger(L,id);lua_pushcclosure(L,closing_finalizer,1);
  lua_setfield(L,-2,"__gc");lua_setmetatable(L,-2);lua_pop(L,1);
}
static int use_peer(lua_State *L) {
  number("peer.callback.top",lua_gettop(owned_peer));
  lua_pushinteger(owned_peer,25);
  lua_getglobal(owned_peer,"number");
  lua_pushinteger(L,lua_tointeger(owned_peer,-1)+lua_tointeger(owned_peer,-2));
  lua_pop(owned_peer,2);
  return 1;
}
static int close_peer(lua_State *L) {
  (void)L;lua_close(owned_peer);owned_peer=NULL;return 0;
}
static int recover_panic(lua_State *L) {
  number("panic.status",lua_status(L));
  number("panic.top",lua_gettop(L));
  string("panic.value",lua_tostring(L,-1));
  longjmp(owned_recovery,1);
  return 0;
}
static int weak_finalizer(lua_State *L) {
  lua_gc(L,LUA_GCCOLLECT,0);
  lua_getglobal(L,"weak_root");lua_rawgeti(L,-1,1);
  number("close.weak.empty",lua_isnil(L,-1));return 0;
}
EXPORT int owned_first(void) {
  owned_main=luaL_newstate();owned_peer=luaL_newstate();
  CHECK(owned_main && owned_peer && owned_main!=owned_peer);
  number("owned.peer.top",lua_gettop(owned_peer));
  number("owned.peer.status",lua_status(owned_peer));
  lua_getglobal(owned_peer,"_G");number("owned.peer.empty",lua_isnil(owned_peer,-1));lua_pop(owned_peer,1);
  lua_pushinteger(owned_peer,17);lua_setglobal(owned_peer,"number");
  lua_pushliteral(owned_peer,"persistent peer prefix");
  number("first.return",root_first(owned_main));
  lua_pushcfunction(owned_main,use_peer);lua_call(owned_main,0,1);
  number("peer.callback.result",lua_tointeger(owned_main,-1));lua_pop(owned_main,1);
  number("peer.after.top",lua_gettop(owned_peer));
  fflush(stdout);return 123;
}
EXPORT int owned_second(void) {
  lua_State *child,*panic_state,*weak_state;
  number("second.return",root_second(owned_main));
  number("peer.retained.top",lua_gettop(owned_peer));
  string("peer.retained.value",lua_tostring(owned_peer,1));
  lua_pushcfunction(owned_main,close_peer);lua_call(owned_main,0,0);
  number("peer.closed",owned_peer==NULL);
  add_closing_userdata(owned_main,1);add_closing_userdata(owned_main,2);add_closing_userdata(owned_main,3);
  lua_getfield(owned_main,LUA_REGISTRYINDEX,"root-child");child=lua_tothread(owned_main,-1);CHECK(child);
  lua_sethook(owned_main,closing_hook,LUA_MASKCALL|LUA_MASKRET,0);
  lua_close(child);owned_main=NULL;
  number("close.finalizers",close_finalizers);number("close.hooks",close_hooks);
  weak_state=luaL_newstate();CHECK(weak_state);luaL_openlibs(weak_state);
  CHECK(luaL_loadstring(weak_state,"weak_root=setmetatable({},{__mode='v'});local t={};weak_root[1]=t;return t")==0);
  lua_call(weak_state,0,1);
  lua_getglobal(weak_state,"weak_root");lua_rawgeti(weak_state,-1,1);
  number("close.weak.before",lua_istable(weak_state,-1));lua_pop(weak_state,2);
  lua_newuserdata(weak_state,1);lua_newtable(weak_state);
  lua_pushcfunction(weak_state,weak_finalizer);lua_setfield(weak_state,-2,"__gc");
  lua_setmetatable(weak_state,-2);lua_pop(weak_state,1);
  lua_close(weak_state);
  panic_state=luaL_newstate();CHECK(panic_state);luaL_openlibs(panic_state);
  lua_atpanic(panic_state,recover_panic);
  if(setjmp(owned_recovery)==0) {
    lua_pushliteral(panic_state,"owned root panic");lua_error(panic_state);CHECK(0);
  }
  number("panic.after.status",lua_status(panic_state));
  lua_settop(panic_state,0);
  CHECK(luaL_loadstring(panic_state,"return 46")==0);
  number("panic.recovery.status",lua_pcall(panic_state,0,1,0));
  number("panic.recovery.value",lua_tointeger(panic_state,1));
  number("panic.retained.status",lua_status(panic_state));
  lua_settop(panic_state,0);
  CHECK(luaL_loadstring(panic_state,"local x=17;escaped=function()return x end;error('owned call panic',0)")==0);
  if(setjmp(owned_recovery)==0) {lua_call(panic_state,0,0);CHECK(0);}
  lua_getglobal(panic_state,"escaped");lua_call(panic_state,0,1);
  number("panic.closed.upvalue",lua_tointeger(panic_state,-1));
  lua_close(panic_state);
  fflush(stdout);return -51;
}
static int returning_panic(lua_State *L) {
  number("returning.panic.status",lua_status(L));
  string("returning.panic.value",lua_tostring(L,-1));
  return 0;
}
EXPORT int owned_default_panic(void) {
  lua_State *L=luaL_newstate();CHECK(L);
  puts("default.panic.before");lua_pushliteral(L,"default owned panic");return lua_error(L);
}
EXPORT int owned_returning_panic(void) {
  lua_State *L=luaL_newstate();CHECK(L);lua_atpanic(L,returning_panic);
  puts("returning.panic.before");lua_pushliteral(L,"returning owned panic");return lua_error(L);
}
#ifdef NATIVE_OWNED_VERIFY
EXPORT int owned_verify(void) {
  static const char expected[]=
#include "native-owned-expected.h"
  ;
  if(strcmp(expected,transcript)==0)return 0;
  fprintf(stderr,"OWNED_TRANSCRIPT_MISMATCH\n%s",transcript);return 1;
}
#endif
#ifdef NATIVE_OWNED_MAIN
int main(int argc,char **argv) {
  if(argc>1) {
    if(strcmp(argv[1],"owned_default_panic")==0)return owned_default_panic();
    if(strcmp(argv[1],"owned_returning_panic")==0)return owned_returning_panic();
    CHECK(0);
  }
  int first=owned_first(),second=owned_second();
  printf("program.first\t%d\nprogram.second\t%d\n",first,second);
  return 0;
}
#endif

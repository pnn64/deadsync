/* Physical stack slots outlive lua_settop until overwritten or traversed. */
static void hook_stack_slots(lua_State *L, lua_Debug *ar) {
  int top=lua_gettop(L),mode,selected;
  lua_getfield(L,LUA_REGISTRYINDEX,"native.hook.stack.target");
  if(lua_isfunction(L,-1)) {
    lua_getinfo(L,"f",ar);selected=lua_rawequal(L,-1,-2);lua_pop(L,2);
    if(!selected)return;
  } else lua_pop(L,1);
  lua_sethook(L,NULL,0,0);
  lua_getfield(L,LUA_REGISTRYINDEX,"native.hook.stack.mode");
  mode=(int)lua_tointeger(L,-1);lua_pop(L,1);
  switch(mode) {
    case 0: lua_settop(L,0);break;
    case 1: lua_settop(L,0);lua_pushinteger(L,71);break;
    case 2: lua_remove(L,1);break;
    case 3: lua_settop(L,1);lua_settop(L,top);break;
    case 4: lua_settop(L,1);lua_pushinteger(L,71);lua_settop(L,0);break;
    case 5: lua_settop(L,0);lua_gc(L,LUA_GCCOLLECT,0);break;
    case 6: lua_settop(L,1);lua_gc(L,LUA_GCCOLLECT,0);break;
    case 7: lua_settop(L,0);lua_gc(L,LUA_GCSTOP,0);break;
    case 8: lua_settop(L,1);lua_pushinteger(L,71);lua_replace(L,1);lua_settop(L,0);break;
    case 9: lua_settop(L,0);lua_gc(L,LUA_GCCOUNT,0);break;
    case 10: lua_settop(L,0);lua_gc(L,LUA_GCSTEP,1000000);break;
    case 11: lua_settop(L,0);lua_pushinteger(L,71);lua_gc(L,LUA_GCCOLLECT,0);break;
    case 12: lua_settop(L,0);lua_pushnil(L);break;
    case 13: lua_settop(L,1);lua_pushnil(L);lua_pushinteger(L,71);lua_remove(L,2);lua_settop(L,0);break;
    case 14: lua_settop(L,0);lua_yield(L,0);return;
    case 15: lua_settop(L,1);lua_pushinteger(L,71);lua_settop(L,0);lua_yield(L,0);return;
  }
}
static int hook_stack_install(lua_State *L) {
  lua_State *co=lua_tothread(L,1);
  int mask=(int)luaL_checkinteger(L,2);
  lua_pushvalue(L,3);lua_setfield(L,LUA_REGISTRYINDEX,"native.hook.stack.mode");
  lua_pushvalue(L,4);lua_setfield(L,LUA_REGISTRYINDEX,"native.hook.stack.target");
  lua_sethook(co,hook_stack_slots,mask,1);return 0;
}
static int hook_stack_echo(lua_State *L) {return lua_gettop(L);}
#ifdef HOOK_STACKS_MAIN
#include "lualib.h"
#include <stdio.h>
EXPORT int luaopen_native(lua_State *L);
int main(int argc,char **argv) {
  lua_State *L=luaL_newstate();int status;
  if(argc!=2 || !L)return 2;
  luaL_openlibs(L);lua_pushcfunction(L,luaopen_native);
  lua_pushliteral(L,"hook stacks");lua_call(L,1,1);lua_setglobal(L,"m");
  status=luaL_loadfile(L,argv[1]);if(!status)status=lua_pcall(L,0,0,0);
  if(status)fprintf(stderr,"%s\n",lua_tostring(L,-1));
  lua_close(L);return status?1:0;
}
#endif

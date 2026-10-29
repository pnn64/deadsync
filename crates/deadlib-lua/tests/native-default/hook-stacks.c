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
  lua_pushinteger(L,top);lua_setfield(L,LUA_REGISTRYINDEX,"native.hook.stack.saved_top");
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
    case 16: lua_settop(L,1);lua_yield(L,0);break;
    case 17: lua_settop(L,0);lua_yield(L,0);break;
    case 18: lua_yield(L,0);break;
    case 19: lua_settop(L,1);lua_yield(L,0);break;
    case 20: lua_settop(L,0);lua_yield(L,0);break;
    case 21: lua_settop(L,2);lua_yield(L,0);break;
    case 22: case 23: lua_settop(L,0);lua_yield(L,0);break;
    default: return;
  }
  if(mode<16)return;
  top=lua_gettop(L);
  lua_pushinteger(L,top);lua_setfield(L,LUA_REGISTRYINDEX,"native.hook.stack.after_yield");
  lua_pushinteger(L,lua_status(L));lua_setfield(L,LUA_REGISTRYINDEX,"native.hook.stack.yield_status");
  if(mode==22 || mode==23) {
    lua_getfield(L,LUA_REGISTRYINDEX,"native.hook.stack.sink");
    lua_pushinteger(L,71);lua_setfield(L,-2,"kept");lua_getfield(L,-1,"kept");
    lua_remove(L,1);lua_settop(L,0);
  }
  else if(mode==19)lua_pushvalue(L,1);
  else if(mode==20)lua_gc(L,LUA_GCCOLLECT,0);
  else {lua_pushinteger(L,71);if(mode==17)lua_pop(L,1);}
}
static int hook_stack_install(lua_State *L) {
  lua_State *co=lua_tothread(L,1);
  int mask=(int)luaL_checkinteger(L,2);
  lua_pushvalue(L,3);lua_setfield(L,LUA_REGISTRYINDEX,"native.hook.stack.mode");
  lua_pushvalue(L,4);lua_setfield(L,LUA_REGISTRYINDEX,"native.hook.stack.target");
  lua_pushvalue(L,5);lua_setfield(L,LUA_REGISTRYINDEX,"native.hook.stack.sink");
  lua_sethook(co,hook_stack_slots,mask,1);return 0;
}
static int hook_stack_echo(lua_State *L) {return lua_gettop(L);}
/* Preserve the yielded public stack without extending the Lua CallInfo top. */
static int hook_stack_resume(lua_State *L) {
  lua_State *co=lua_tothread(L,1);int args=lua_gettop(L)-1,status,top,first,i;
  if(!co || co==L)return luaL_error(L,"another thread expected");
  lua_setlevel(L,co);lua_xmove(L,co,args);status=lua_resume(co,args);top=lua_gettop(co);
  if(!lua_checkstack(L,2*top+5))return luaL_error(L,"snapshot reserve");
  first=lua_gettop(L)+1;lua_xmove(co,L,top);
  for(i=0;i<top;i++)lua_pushvalue(L,first+i);
  lua_xmove(L,co,top);
  lua_pushinteger(L,status);lua_pushinteger(L,top);lua_createtable(L,top,0);
  for(i=0;i<top;i++){lua_pushvalue(L,first+i);lua_rawseti(L,-2,i+1);}
  return 3;
}
static int hook_stack_clear(lua_State *L) {
  lua_State *co=lua_tothread(L,1);if(!co || co==L)return luaL_error(L,"another thread expected");
  lua_settop(co,0);return 0;
}
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

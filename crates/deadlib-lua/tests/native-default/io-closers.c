/* Direct close callbacks inspect their caller's C activation. */
#include <string.h>
static void closer_capture(lua_State *L,int index,const char *field) {
  if(lua_type(L,lua_upvalueindex(index))>=0)lua_pushvalue(L,lua_upvalueindex(index));
  else lua_pushnil(L);
  lua_setfield(L,-2,field);
}
static int closer_observe(lua_State *L) {
  int top=lua_gettop(L),i;lua_Debug ar;
  lua_createtable(L,top,5);
  lua_pushinteger(L,top);lua_setfield(L,-2,"n");
  for(i=1;i<=top;i++){lua_pushvalue(L,i);lua_rawseti(L,-2,i);}
  lua_pushvalue(L,LUA_ENVIRONINDEX);lua_setfield(L,-2,"environment");
  closer_capture(L,1,"first");closer_capture(L,2,"second");
  if(lua_getstack(L,0,&ar)&&lua_getinfo(L,"f",&ar))lua_setfield(L,-2,"function");
  lua_pushvalue(L,-1);lua_setfield(L,LUA_GLOBALSINDEX,"closer_observation");
  return 1;
}
static int closer_results(lua_State *L) {
  closer_observe(L);lua_settop(L,-2);
  lua_pushinteger(L,17);lua_pushnil(L);lua_pushboolean(L,0);return 3;
}
static int closer_replace(lua_State *L) {
  closer_observe(L);lua_settop(L,-2);lua_settop(L,0);lua_pushinteger(L,29);return 1;
}
static int closer_forward(lua_State *L) {
  lua_CFunction function;
  closer_observe(L);lua_settop(L,-2);lua_getfenv(L,1);lua_getfield(L,-1,"original");
  function=lua_tocfunction(L,-1);if(!function)return luaL_error(L,"original close function missing");
  return function(L);
}
static int closer_error(lua_State *L) {
  closer_observe(L);lua_settop(L,-2);lua_getfenv(L,1);lua_getfield(L,-1,"marker");return lua_error(L);
}
static int closer_erase(lua_State *L) {
  lua_getfenv(L,1);lua_pushnil(L);lua_setfield(L,-2,"__close");
  lua_settop(L,0);lua_gc(L,LUA_GCCOLLECT,0);
  lua_getfield(L,LUA_GLOBALSINDEX,"closer_erased");return 1;
}
static int closer_yield_tail(lua_State *L) {lua_pushinteger(L,41);return lua_yield(L,1);}
static int closer_yield_zero(lua_State *L) {return lua_yield(L,0);}
static int file_closer_capture(lua_State *L) {
  return lua_getupvalue(L,1,(int)luaL_checkinteger(L,2))?1:0;
}
static int file_body_call(lua_State *L) {
  lua_CFunction function=lua_tocfunction(L,1);
  if(!function)return luaL_error(L,"C function expected");
  lua_remove(L,1);return function(L);
}
static int file_body(lua_State *L) {
  lua_pushvalue(L,1);lua_pushvalue(L,2);lua_pushcclosure(L,file_body_call,2);return 1;
}
static void file_close_hook(lua_State *L,lua_Debug *event) {
  int top=lua_gettop(L),matches=0;lua_Debug frame;lua_CFunction function;
  (void)event;
  if(lua_getstack(L,0,&frame)&&lua_getinfo(L,"f",&frame)){
    lua_getfield(L,LUA_REGISTRYINDEX,"file.close.hook.target");matches=lua_rawequal(L,-1,-2);
  }
  lua_settop(L,top);if(!matches)return;
  lua_sethook(L,NULL,0,0);lua_getfield(L,LUA_REGISTRYINDEX,"file.close.hook.body");
  function=lua_tocfunction(L,-1);lua_settop(L,top);
  if(function)function(L);
}
static int file_hook(lua_State *L) {
  lua_State *thread=lua_tothread(L,3);if(!thread)thread=L;
  lua_pushvalue(L,1);lua_setfield(L,LUA_REGISTRYINDEX,"file.close.hook.body");
  lua_pushvalue(L,2);lua_setfield(L,LUA_REGISTRYINDEX,"file.close.hook.target");
  lua_sethook(thread,file_close_hook,LUA_MASKLINE,0);return 0;
}
static int file_closer(lua_State *L) {
  const char *mode=luaL_checklstring(L,1,NULL);lua_CFunction function=NULL;
  if(!strcmp(mode,"observe"))function=closer_observe;
  else if(!strcmp(mode,"results"))function=closer_results;
  else if(!strcmp(mode,"replace"))function=closer_replace;
  else if(!strcmp(mode,"forward"))function=closer_forward;
  else if(!strcmp(mode,"error"))function=closer_error;
  else if(!strcmp(mode,"erase"))function=closer_erase;
  else if(!strcmp(mode,"yield-tail"))function=closer_yield_tail;
  else if(!strcmp(mode,"yield-zero"))function=closer_yield_zero;
  else return luaL_error(L,"unknown closer mode");
  lua_pushvalue(L,2);lua_pushcclosure(L,function,1);return 1;
}

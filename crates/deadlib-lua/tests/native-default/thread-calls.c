/* Shared public C API fixture for calls through a selected thread. */
static int inactive_yield_tail(lua_State *L) {return lua_yield(L,1);}
static int inactive_called(lua_State *L) {lua_pushthread(L);lua_pushinteger(L,lua_status(L));return 2;}
static int inactive_cpcall_body(lua_State *L) {
  int *fail=(int *)lua_touserdata(L,1);
  if(!fail)return luaL_error(L,"missing C call data");
  if(*fail==2){lua_pushinteger(L,17);return lua_yield(L,1);}
  if(*fail){lua_pushvalue(L,LUA_GLOBALSINDEX);return lua_error(L);}
  return 0;
}
static int inactive_cpcall(lua_State *L) {
  lua_State *co=lua_tothread(L,1);int mode=lua_type(L,2)==1?lua_toboolean(L,2):(int)lua_tointeger(L,2),before,status,after;
  if(!co || co==L)return luaL_error(L,"another thread expected");
  before=lua_gettop(co);status=lua_cpcall(co,inactive_cpcall_body,&mode);after=lua_gettop(co);
  lua_pushinteger(L,status);lua_pushinteger(L,before);lua_pushinteger(L,after);lua_pushinteger(L,lua_status(co));
  if(status)lua_xmove(co,L,1);else lua_pushnil(L);
  lua_settop(co,before);return 5;
}
static int inactive_pcall(lua_State *L) {
  lua_State *co=lua_tothread(L,1);
  int before,status,after,oldstatus,i,nargs=lua_gettop(L)-4,handler=0;
  int wanted=(int)lua_tointeger(L,3);
  if(!co || co==L)return luaL_error(L,"another thread expected");
  before=lua_gettop(co);oldstatus=lua_status(co);
  if(!lua_checkstack(co,nargs+8))return luaL_error(L,"target stack reserve");
  if(lua_type(L,4)!=0){lua_pushvalue(L,4);lua_xmove(L,co,1);handler=before+1;}
  lua_pushvalue(L,2);lua_xmove(L,co,1);
  for(i=5;i<=nargs+4;i++){lua_pushvalue(L,i);lua_xmove(L,co,1);}
  status=lua_pcall(co,nargs,wanted,handler);after=lua_gettop(co);
  lua_pushinteger(L,status);lua_createtable(L,after,0);
  for(i=1;i<=after;i++){lua_pushvalue(co,i);lua_xmove(co,L,1);lua_rawseti(L,-2,i);}
  lua_pushinteger(L,before);lua_pushinteger(L,after);lua_pushinteger(L,oldstatus);lua_pushinteger(L,lua_status(co));
  lua_settop(co,before);return 6;
}

/* A nonlocal Lua error follows the selected state's protection chain. */
static int waiting_raise(lua_State *L) {
  lua_State *target=lua_tothread(L,1);
  if(!target || target==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(target,1))return luaL_error(L,"waiting target reserve");
  lua_pushvalue(L,2);lua_xmove(L,target,1);return lua_error(target);
}
static int waiting_snapshot(lua_State *L) {
  lua_State *target=lua_tothread(L,1);lua_Debug ar;int count=0,top,status;char kinds[65];
  if(!target)return luaL_error(L,"thread expected");
  top=lua_gettop(target);status=lua_status(target);
  while(count<64 && lua_getstack(target,count,&ar)) {
    if(!lua_getinfo(target,"S",&ar))return luaL_error(L,"snapshot info failed");
    kinds[count]=ar.what[0];count++;
  }
  kinds[count]=0;
  lua_pushinteger(L,status);lua_pushinteger(L,top);lua_pushinteger(L,count);lua_pushstring(L,kinds);return 4;
}
static int waiting_c_resume(lua_State *L) {
  lua_State *target=lua_tothread(L,1);int result;
  if(!target)return luaL_error(L,"thread expected");
  result=lua_resume(target,0);lua_pushinteger(L,result);return 1;
}

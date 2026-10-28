/* Included by both the public-header module and the differential C fixture. */
static int inactive_captures(lua_State *L) {
  lua_pushvalue(L,lua_upvalueindex(1));lua_pushvalue(L,lua_upvalueindex(2));
  lua_pushvalue(L,LUA_ENVIRONINDEX);return 3;
}
static int inactive_objects(lua_State *L) {
  lua_State *co=lua_tothread(L,1);
  int base,table,userdata,closure,source,summary,top,entries=0,total=0;
  const char *name;
  if(!co || co==L)return luaL_error(L,"another thread expected");
  base=lua_gettop(co);
  if(!lua_checkstack(co,300))return luaL_error(L,"stack reserve failed");
  /* table keys use raw operations even in the presence of throwing metatables. */
  lua_createtable(co,3,8);table=base+1;
  lua_pushvalue(L,2);lua_xmove(L,co,1);lua_setmetatable(co,table);
  lua_pushstring(co,"a");lua_pushinteger(co,17);lua_rawset(co,table);
  lua_pushinteger(co,25);lua_rawseti(co,table,1);
  lua_pushinteger(co,11);lua_rawseti(co,-2,2);
  lua_pushstring(co,"a");lua_rawget(co,table);total+=(int)lua_tointeger(co,-1);lua_settop(co,-2);
  lua_rawgeti(co,table,1);total+=(int)lua_tointeger(co,-1);lua_settop(co,-2);
  lua_pushnil(co);while(lua_next(co,table)){entries++;lua_settop(co,-2);}
  if(lua_getmetatable(co,table)){lua_pushvalue(L,2);lua_xmove(L,co,1);if(!lua_rawequal(co,-1,-2))return luaL_error(L,"metatable identity");lua_settop(co,-3);}
  else return luaL_error(L,"missing metatable");
  lua_pushnil(co);lua_setmetatable(co,table);
  if(lua_getmetatable(co,table))return luaL_error(L,"metatable not cleared");
  lua_pushvalue(L,2);lua_xmove(L,co,1);lua_setmetatable(co,table);
  *(lua_Integer *)lua_newuserdata(co,sizeof(lua_Integer))=57;userdata=base+2;
  lua_getfenv(co,userdata);lua_pushvalue(L,5);lua_xmove(L,co,1);
  if(!lua_rawequal(co,-1,-2))return luaL_error(L,"userdata inherited environment");lua_settop(co,-3);
  lua_pushvalue(L,3);lua_xmove(L,co,1);if(!lua_setfenv(co,userdata))return luaL_error(L,"userdata environment");
  lua_getfenv(co,userdata);lua_pushvalue(L,3);lua_xmove(L,co,1);
  if(!lua_rawequal(co,-1,-2))return luaL_error(L,"userdata environment identity");lua_settop(co,-3);
  lua_pushinteger(co,17);lua_pushvalue(L,3);lua_xmove(L,co,1);
  lua_pushcclosure(co,inactive_captures,2);closure=base+3;
  lua_getfenv(co,closure);lua_pushvalue(L,5);lua_xmove(L,co,1);
  if(!lua_rawequal(co,-1,-2))return luaL_error(L,"closure inherited environment");lua_settop(co,-3);
  name=lua_getupvalue(co,closure,1);if(!name || name[0] || lua_tointeger(co,-1)!=17)return luaL_error(L,"C capture read");lua_settop(co,-2);
  lua_pushinteger(co,29);name=lua_setupvalue(co,closure,1);if(!name || name[0])return luaL_error(L,"C capture write");
  lua_pushinteger(co,99);top=lua_gettop(co);
  if(lua_setupvalue(co,closure,3)!=NULL || lua_gettop(co)!=top || lua_getupvalue(co,closure,0)!=NULL || lua_gettop(co)!=top)return luaL_error(L,"invalid capture stack");lua_settop(co,-2);
  lua_pushvalue(L,3);lua_xmove(L,co,1);if(!lua_setfenv(co,closure))return luaL_error(L,"closure environment");
  lua_pushvalue(L,4);lua_xmove(L,co,1);source=base+4;
  name=lua_getupvalue(co,source,1);if(!name || !name[0] || lua_tointeger(co,-1)!=101)return luaL_error(L,"Lua capture read");lua_settop(co,-2);
  lua_pushinteger(co,103);if(!lua_setupvalue(co,source,1))return luaL_error(L,"Lua capture write");
  /* Only the inactive stack roots the newly allocated table/userdata/closure. */
  lua_gc(L,2,0);lua_gc(L,2,0);
  lua_createtable(co,0,0);summary=base+5;
  lua_pushinteger(co,entries);lua_rawseti(co,summary,1);
  lua_pushinteger(co,total);lua_rawseti(co,summary,2);
  lua_pushinteger(co,base);lua_rawseti(co,summary,3);
  lua_pushinteger(co,lua_status(co));lua_rawseti(co,summary,4);
  lua_pushinteger(co,*(lua_Integer *)lua_touserdata(co,userdata));lua_rawseti(co,summary,5);
  lua_pushboolean(co,lua_tocfunction(co,closure)==inactive_captures);lua_rawseti(co,summary,6);
  lua_pushvalue(L,3);lua_xmove(L,co,1);top=lua_gettop(co);
  if(lua_setfenv(co,table) || lua_gettop(co)!=top-1)return luaL_error(L,"unsupported environment stack");
  lua_xmove(co,L,5);
  if(lua_gettop(co)!=base)return luaL_error(L,"target stack prefix");
  return 5;
}

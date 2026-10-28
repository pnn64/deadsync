/* File loading on another thread through the public Lua 5.1 API. */
static int inactive_loadfile(lua_State *L) {
  lua_State *co=lua_tothread(L,1);
  const char *path=lua_type(L,2)==0?NULL:luaL_checklstring(L,2,NULL);
  int before,after,old,status,i;
  if(!co || co==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(L,16)||!lua_checkstack(co,8))return luaL_error(L,"file fixture stack reserve");
  before=lua_gettop(co);old=lua_status(co);
  status=luaL_loadfile(co,path);after=lua_gettop(co);
  lua_pushinteger(L,status);lua_pushinteger(L,before);lua_pushinteger(L,after);
  lua_pushinteger(L,old);lua_pushinteger(L,lua_status(co));
  lua_pushvalue(co,-1);lua_xmove(co,L,1);
  lua_createtable(L,after,0);
  for(i=1;i<=after;i++){lua_pushvalue(co,i);lua_xmove(co,L,1);lua_rawseti(L,-2,i);}
  lua_settop(co,before);return 7;
}
/* Included after panic.c so recovery stays entirely below the Rust boundary. */
static int inactive_filepanic(lua_State *L) {
  lua_State *co=lua_tothread(L,1);const char *path=luaL_checklstring(L,3,NULL);PanicTrap *trap;
  if(!co || co==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(L,16)||!lua_checkstack(co,8))return luaL_error(L,"file panic stack reserve");
  trap=(PanicTrap *)calloc(1,sizeof(PanicTrap));if(!trap)return luaL_error(L,"file panic fixture allocation");
  trap->caller=L;trap->target=co;trap->previous=panic_trap;panic_trap=trap;
  if(!setjmp(trap->escape)) {
    lua_gc(L,LUA_GCSETSTEPMUL,0);lua_gc(L,LUA_GCRESTART,0);
    /* Reserve without a GC check so the next check occurs on the target. */
    if(!lua_checkstack(co,4000))abort();
    luaL_loadfile(co,path);
  }
  panic_trap=trap->previous;
  lua_pushinteger(L,trap->callback);lua_pushinteger(L,trap->status);lua_pushinteger(L,trap->top);
  lua_pushinteger(L,trap->type);lua_pushboolean(L,trap->same);lua_pushboolean(L,trap->identity);
  lua_pushinteger(L,trap->caller_status);lua_pushinteger(L,trap->has_frame);
  lua_pushinteger(L,lua_gettop(co));lua_pushinteger(L,lua_status(co));
  free(trap);return 10;
}

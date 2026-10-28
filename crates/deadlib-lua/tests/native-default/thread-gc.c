/* Explicit collector operations through another thread's public C stack. */
static int inactive_gc(lua_State *L) {
  lua_State *target=lua_tothread(L,1);int mode=(int)luaL_checkinteger(L,2);
  int data=(int)luaL_checkinteger(L,3),before,old,result,after;
  if(!target || target==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(L,8))return luaL_error(L,"GC caller reserve");
  before=lua_gettop(target);old=lua_status(target);
  result=lua_gc(target,mode,data);after=lua_gettop(target);
  lua_pushinteger(L,result);lua_pushinteger(L,before);lua_pushinteger(L,after);
  lua_pushinteger(L,old);lua_pushinteger(L,lua_status(target));return 5;
}
static int inactive_gc_panic(lua_State *L) {
  lua_State *target=lua_tothread(L,1);PanicTrap *trap;lua_CFunction previous;
  int mode=(int)luaL_checkinteger(L,3),data=(int)luaL_checkinteger(L,4);
  if(!target || target==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(L,16))return luaL_error(L,"GC panic caller reserve");
  trap=(PanicTrap *)calloc(1,sizeof(PanicTrap));if(!trap)return luaL_error(L,"GC panic reserve");
  trap->caller=L;trap->target=target;trap->previous=panic_trap;panic_trap=trap;
  previous=lua_atpanic(target,panic_hook_a);
  if(!setjmp(trap->escape))lua_gc(target,mode,data);
  panic_trap=trap->previous;lua_atpanic(target,previous);
  lua_pushinteger(L,trap->callback);lua_pushinteger(L,trap->status);lua_pushinteger(L,trap->top);
  lua_pushinteger(L,trap->type);lua_pushboolean(L,trap->same);lua_pushboolean(L,trap->identity);
  lua_pushinteger(L,trap->caller_status);lua_pushinteger(L,trap->has_frame);
  lua_pushinteger(L,lua_gettop(target));lua_pushinteger(L,lua_status(target));
  free(trap);return 10;
}

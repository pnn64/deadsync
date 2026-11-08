/* Calls through another state must not install lua_pcall's error handler. */
static int inactive_call(lua_State *L) {
  lua_State *target=lua_tothread(L,1);
  int before,after,old,i,nargs=lua_gettop(L)-3,wanted=(int)lua_tointeger(L,3);
  if(!target || target==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(target,nargs+8))return luaL_error(L,"call target reserve");
  before=lua_gettop(target);old=lua_status(target);
  lua_pushvalue(L,2);lua_xmove(L,target,1);
  for(i=4;i<=nargs+3;i++){lua_pushvalue(L,i);lua_xmove(L,target,1);}
  lua_call(target,nargs,wanted);after=lua_gettop(target);
  lua_pushinteger(L,before);lua_pushinteger(L,after);lua_pushinteger(L,old);lua_pushinteger(L,lua_status(target));
  lua_createtable(L,after,0);
  for(i=1;i<=after;i++){lua_pushvalue(target,i);lua_xmove(target,L,1);lua_rawseti(L,-2,i);}
  lua_settop(target,before);return 5;
}
static int inactive_call_panic(lua_State *L) {
  lua_State *target=lua_tothread(L,1);PanicTrap *trap;lua_CFunction previous;
  if(!target || target==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(L,16)||!lua_checkstack(target,8))return luaL_error(L,"call panic reserve");
  trap=(PanicTrap *)calloc(1,sizeof(PanicTrap));if(!trap)return luaL_error(L,"call panic trap reserve");
  trap->caller=L;trap->target=target;trap->previous=panic_trap;panic_trap=trap;
  previous=lua_atpanic(target,panic_hook_a);
  lua_pushvalue(L,3);lua_xmove(L,target,1);
  if(!setjmp(trap->escape))lua_call(target,0,-1);
  panic_trap=trap->previous;lua_atpanic(target,previous);
  lua_pushinteger(L,trap->callback);lua_pushinteger(L,trap->status);lua_pushinteger(L,trap->top);
  lua_pushinteger(L,trap->type);lua_pushboolean(L,trap->same);lua_pushboolean(L,trap->identity);
  lua_pushinteger(L,trap->caller_status);lua_pushinteger(L,trap->has_frame);
  lua_pushinteger(L,lua_gettop(target));lua_pushinteger(L,lua_status(target));
  free(trap);return 10;
}

/* Observe yielded table APIs without reading below their reported base. */
static int selected_table_yield_all(lua_State *L) {return lua_yield(L,lua_gettop(L));}
static int selected_table_yield_tail(lua_State *L) {return lua_yield(L,1);}
static int selected_table_yield_zero(lua_State *L) {return lua_yield(L,0);}
static int selected_table_yield(lua_State *L) {
  lua_State *target=lua_tothread(L,1);PanicTrap *trap;lua_CFunction previous;
  int mode=(int)luaL_checkinteger(L,3),before,old,after,i;
  volatile int result=0;
  if(!target || target==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(L,24)||!lua_checkstack(target,8))return luaL_error(L,"table yield reserve");
  before=lua_gettop(target);old=lua_status(target);
  trap=(PanicTrap *)calloc(1,sizeof(PanicTrap));if(!trap)return luaL_error(L,"table yield trap reserve");
  trap->caller=L;trap->target=target;trap->previous=panic_trap;panic_trap=trap;
  previous=lua_atpanic(target,panic_hook_a);
  if(!setjmp(trap->escape))result=selected_table_operation(L,target,mode,4);
  panic_trap=trap->previous;lua_atpanic(target,previous);after=lua_gettop(target);
  lua_pushinteger(L,trap->callback);lua_pushinteger(L,trap->status);lua_pushinteger(L,trap->top);
  lua_pushinteger(L,trap->type);lua_pushboolean(L,trap->same);lua_pushboolean(L,trap->identity);
  lua_pushinteger(L,trap->caller_status);lua_pushinteger(L,trap->has_frame);
  lua_pushinteger(L,after);lua_pushinteger(L,lua_status(target));
  lua_pushinteger(L,before);lua_pushinteger(L,old);lua_pushboolean(L,result);
  lua_createtable(L,after>0?after:0,0);
  for(i=1;i<=after;i++) {
    lua_pushvalue(target,i);lua_xmove(target,L,1);lua_rawseti(L,-2,i);
  }
  free(trap);return 14;
}

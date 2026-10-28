/* Allocating and executing through another thread's public stack. */
static int inactive_construct(lua_State *L) {
  lua_State *target=lua_tothread(L,1),*created;int before,after,old,same,env;
  if(!target || target==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(target,8))return luaL_error(L,"construct target reserve");
  before=lua_gettop(target);old=lua_status(target);
  created=lua_newthread(target);after=lua_gettop(target);
  same=created==lua_tothread(target,-1) && created==lua_topointer(target,-1);
  lua_pushvalue(target,LUA_GLOBALSINDEX);lua_getfenv(target,-2);env=lua_rawequal(target,-1,-2);lua_settop(target,-3);
  lua_pushinteger(L,before);lua_pushinteger(L,after);lua_pushinteger(L,old);lua_pushinteger(L,lua_status(target));
  lua_pushboolean(L,same);lua_pushboolean(L,env);lua_xmove(target,L,1);lua_settop(target,before);return 7;
}
static int inactive_format(lua_State *L) {
  lua_State *target=lua_tothread(L,1);int before,after,old;
  if(!target || target==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(target,32))return luaL_error(L,"format target reserve");
  before=lua_gettop(target);old=lua_status(target);
  lua_pushfstring(target,"%s:%d:%f:%c:%%","value",17,3.5,65);after=lua_gettop(target);
  lua_pushinteger(L,before);lua_pushinteger(L,after);lua_pushinteger(L,old);lua_pushinteger(L,lua_status(target));
  lua_xmove(target,L,1);lua_settop(target,before);return 5;
}
static int inactive_concat(lua_State *L) {
  lua_State *target=lua_tothread(L,1);int before,after,old,i,count=lua_gettop(L)-1;
  if(!target || target==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(target,count+8))return luaL_error(L,"concat target reserve");
  before=lua_gettop(target);old=lua_status(target);
  for(i=2;i<=count+1;i++){lua_pushvalue(L,i);lua_xmove(L,target,1);}
  lua_concat(target,count);after=lua_gettop(target);
  lua_pushinteger(L,before);lua_pushinteger(L,after);lua_pushinteger(L,old);lua_pushinteger(L,lua_status(target));
  if(after>0){lua_pushvalue(target,-1);lua_xmove(target,L,1);}else lua_pushnil(L);
  lua_settop(target,before);return 5;
}
static int inactive_concat_panic(lua_State *L) {
  lua_State *target=lua_tothread(L,1);PanicTrap *trap;lua_CFunction previous;
  int i,count=lua_gettop(L)-2;
  if(!target || target==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(L,16)||!lua_checkstack(target,count+8))return luaL_error(L,"concat panic reserve");
  for(i=3;i<=count+2;i++){lua_pushvalue(L,i);lua_xmove(L,target,1);}
  trap=(PanicTrap *)calloc(1,sizeof(PanicTrap));if(!trap)return luaL_error(L,"concat trap reserve");
  trap->caller=L;trap->target=target;trap->previous=panic_trap;panic_trap=trap;
  previous=lua_atpanic(target,panic_hook_a);
  if(!setjmp(trap->escape))lua_concat(target,count);
  panic_trap=trap->previous;lua_atpanic(target,previous);
  lua_pushinteger(L,trap->callback);lua_pushinteger(L,trap->status);lua_pushinteger(L,trap->top);
  lua_pushinteger(L,trap->type);lua_pushboolean(L,trap->same);lua_pushboolean(L,trap->identity);
  lua_pushinteger(L,trap->caller_status);lua_pushinteger(L,trap->has_frame);
  lua_pushinteger(L,lua_gettop(target));lua_pushinteger(L,lua_status(target));
  free(trap);return 10;
}
static int inactive_stack_offsets(lua_State *L) {
  lua_State *target=lua_tothread(L,1);int before,one,two,three,status;
  if(!target || target==L)return luaL_error(L,"another thread expected");
  before=lua_gettop(target);status=lua_status(target);
  if(!lua_checkstack(target,8))return luaL_error(L,"offset target reserve");
  lua_pushnil(target);one=lua_gettop(target);
  lua_pushinteger(target,59);two=lua_gettop(target);
  lua_pushboolean(target,1);three=lua_gettop(target);
  if(three>0 && (lua_type(target,-1)!=1||!lua_toboolean(target,-1)))abort();
  lua_settop(target,0);
  lua_pushinteger(L,before);lua_pushinteger(L,one);lua_pushinteger(L,two);lua_pushinteger(L,three);
  lua_pushinteger(L,status);lua_pushinteger(L,lua_status(target));lua_pushinteger(L,lua_gettop(target));
  return 7;
}

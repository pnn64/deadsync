/* Metamethod-capable public APIs on another thread's stack. */
static int selected_table_operation(lua_State *L,lua_State *target,int mode,int first) {
  int result=0;
  const char *field=NULL;
  if(mode==1 || mode==3)field=luaL_checklstring(L,first+1,NULL);
  lua_pushvalue(L,first);lua_xmove(L,target,1);
  if(mode==0 || mode==2 || mode==4 || mode==5) {
    lua_pushvalue(L,first+1);lua_xmove(L,target,1);
  }
  if(mode==2 || mode==3){lua_pushvalue(L,first+2);lua_xmove(L,target,1);}
  switch(mode) {
    case 0:lua_gettable(target,-2);break;
    case 1:lua_getfield(target,-1,field);break;
    case 2:lua_settable(target,-3);break;
    case 3:lua_setfield(target,-2,field);break;
    case 4:result=lua_equal(target,-2,-1);break;
    case 5:result=lua_lessthan(target,-2,-1);break;
    default:return luaL_error(L,"invalid table operation");
  }
  return result;
}
static int selected_table(lua_State *L) {
  lua_State *target=lua_tothread(L,1);
  int mode=(int)luaL_checkinteger(L,2),before,old,after,result;
  if(!target || target==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(L,10)||!lua_checkstack(target,8))return luaL_error(L,"table stack reserve");
  before=lua_gettop(target);old=lua_status(target);
  result=selected_table_operation(L,target,mode,3);after=lua_gettop(target);
  lua_pushinteger(L,before);lua_pushinteger(L,after);lua_pushinteger(L,old);lua_pushinteger(L,lua_status(target));
  if(mode<2){lua_pushvalue(target,-1);lua_xmove(target,L,1);}
  else if(mode>3)lua_pushboolean(L,result);else lua_pushnil(L);
  lua_settop(target,before);return 5;
}
static int selected_table_panic(lua_State *L) {
  lua_State *target=lua_tothread(L,1);PanicTrap *trap;lua_CFunction previous;
  int mode=(int)luaL_checkinteger(L,3);
  if(!target || target==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(L,16)||!lua_checkstack(target,8))return luaL_error(L,"table panic reserve");
  trap=(PanicTrap *)calloc(1,sizeof(PanicTrap));if(!trap)return luaL_error(L,"table panic allocation");
  trap->caller=L;trap->target=target;trap->previous=panic_trap;panic_trap=trap;
  previous=lua_atpanic(target,panic_hook_a);
  if(!setjmp(trap->escape))selected_table_operation(L,target,mode,4);
  panic_trap=trap->previous;lua_atpanic(target,previous);
  lua_pushinteger(L,trap->callback);lua_pushinteger(L,trap->status);lua_pushinteger(L,trap->top);
  lua_pushinteger(L,trap->type);lua_pushboolean(L,trap->same);lua_pushboolean(L,trap->identity);
  lua_pushinteger(L,trap->caller_status);lua_pushinteger(L,trap->has_frame);
  lua_pushinteger(L,lua_gettop(target));lua_pushinteger(L,lua_status(target));
  free(trap);return 10;
}

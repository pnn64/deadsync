/* Leave yielded values and pending calls on the selected public stack. */
static int selected_yield_call(lua_State *L) {
  lua_State *target=lua_tothread(L,1);
  int before,after,old,i,status=0,handler=0,nargs=lua_gettop(L)-5;
  int wanted=(int)lua_tointeger(L,3),protected_call=lua_toboolean(L,4);
  if(!target || target==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(target,nargs+12))return luaL_error(L,"yield call target reserve");
  before=lua_gettop(target);old=lua_status(target);
  if(protected_call && lua_type(L,5)!=0){lua_pushvalue(L,5);lua_xmove(L,target,1);handler=before+1;}
  lua_pushvalue(L,2);lua_xmove(L,target,1);
  for(i=6;i<=nargs+5;i++){lua_pushvalue(L,i);lua_xmove(L,target,1);}
  if(protected_call)status=lua_pcall(target,nargs,wanted,handler);
  else lua_call(target,nargs,wanted);
  after=lua_gettop(target);
  lua_pushinteger(L,status);lua_pushinteger(L,before);lua_pushinteger(L,after);
  lua_pushinteger(L,old);lua_pushinteger(L,lua_status(target));lua_createtable(L,after,0);
  for(i=1;i<=after;i++){lua_pushvalue(target,i);lua_xmove(target,L,1);lua_rawseti(L,-2,i);}
  return 6;
}
static int selected_yield_resume(lua_State *L) {
  lua_State *target=lua_tothread(L,1);int nargs=lua_gettop(L)-1,status,nresults;
  if(!target || target==L)return luaL_error(L,"another thread expected");
  if(!lua_checkstack(target,nargs+8))return luaL_error(L,"resume target reserve");
  lua_setlevel(L,target);lua_xmove(L,target,nargs);status=lua_resume(target,nargs);
  nresults=(status==0||status==1)?lua_gettop(target):1;
  if(!lua_checkstack(L,nresults+1))return luaL_error(L,"resume result reserve");
  lua_pushboolean(L,status==0||status==1);lua_xmove(target,L,nresults);return nresults+1;
}

/* Allocation checks use the public API and leave their timing observable. */
static int allocation_gc_body(lua_State *L) {lua_pushvalue(L,lua_upvalueindex(1));return 1;}
static int allocation_gc(lua_State *L) {
  int mode=(int)luaL_checkinteger(L,1),top,type,before,ran;
  lua_State *target=lua_tothread(L,4);PanicTrap *trap=NULL;lua_CFunction previous=NULL;
  if(!target)target=L;
  if(!lua_checkstack(L,16))return luaL_error(L,"allocation fixture reserve");
  if(!lua_checkstack(target,8))return luaL_error(L,"allocation operand reserve");
  before=lua_gettop(target);
  if(mode==4){lua_pushvalue(L,2);if(target!=L)lua_xmove(L,target,1);}
  if(mode==6){lua_pushstring(target,"left");lua_pushstring(target,"right");}
  if(mode==7 || mode==9 || mode==10)lua_pushinteger(target,17);
  if(target!=L) {
    trap=(PanicTrap *)calloc(1,sizeof(PanicTrap));if(!trap)return luaL_error(L,"allocation trap reserve");
    trap->caller=L;trap->target=target;trap->previous=panic_trap;panic_trap=trap;
    previous=lua_atpanic(target,panic_hook_a);
  }
  if(!trap || !setjmp(trap->escape)) {
  lua_gc(L,LUA_GCSETSTEPMUL,0);lua_gc(L,LUA_GCRESTART,0);
  if(!lua_checkstack(target,4000))abort();
  switch(mode) {
    case 0:lua_pushstring(target,"allocation-string");break;
    case 1:lua_pushlstring(target,"a\0b",3);break;
    case 2:lua_createtable(target,0,0);break;
    case 3:lua_newuserdata(target,1);break;
    case 4:lua_pushcclosure(target,allocation_gc_body,1);break;
    case 5:lua_pushfstring(target,"%s:%d","allocation",17);break;
    case 6:lua_concat(target,2);break;
    case 7:lua_tolstring(target,-1,NULL);break;
    case 8:lua_concat(target,0);break;
    case 9:lua_concat(target,1);break;
    case 10:lua_objlen(target,-1);break;
    case 11:lua_newthread(target);break;
    case 12:lua_pushnil(target);break;
    default:return luaL_error(L,"unknown allocation mode");
  }
  }
  top=lua_gettop(target);type=lua_type(target,-1);
  lua_getfield(L,3,"ran");ran=lua_toboolean(L,-1);lua_settop(L,-2);
  if(trap) {
    if(trap->callback && !(trap->same && trap->identity && trap->status==2 && trap->top==1))abort();
    panic_trap=trap->previous;lua_atpanic(target,previous);free(trap);
    lua_settop(target,before);
  }
  lua_pushboolean(L,ran);lua_pushinteger(L,top);lua_pushinteger(L,type);return 3;
}

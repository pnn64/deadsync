/* Protected parsing on another thread, using only the public Lua API. */
typedef struct ThreadReader {
  lua_State *caller;
  const char *bytes;
  size_t length, offset, block;
  int visits;
} ThreadReader;
static const char *thread_reader(lua_State *L,void *data,size_t *size) {
  ThreadReader *r=(ThreadReader *)data;
  if(!lua_checkstack(L,8)){luaL_error(L,"reader stack reserve");return NULL;}
  r->visits++;
  if(lua_type(r->caller,3)==6) {
    lua_pushvalue(r->caller,3);lua_xmove(r->caller,L,1);
    lua_pushinteger(L,r->visits);lua_pushinteger(L,lua_status(L));
    lua_call(L,2,0);
  }
  if(lua_type(r->caller,6)==5) {
    lua_Debug ar;
    /* The base CallInfo has no current function for LUA_ENVIRONINDEX. */
    lua_pushvalue(L,lua_getstack(L,0,&ar)?LUA_ENVIRONINDEX:LUA_GLOBALSINDEX);lua_xmove(L,r->caller,1);
    lua_rawseti(r->caller,6,r->visits);
  }
  *size=r->length-r->offset;
  if(*size>r->block)*size=r->block;
  if(!*size)return NULL;
  {const char *p=r->bytes+r->offset;r->offset+=*size;return p;}
}
static int inactive_load(lua_State *L) {
  lua_State *co=lua_tothread(L,1);ThreadReader reader;
  int before,after,old,status;
  const char *name=lua_type(L,4)==0?NULL:luaL_checklstring(L,4,NULL);
  if(!co || co==L)return luaL_error(L,"another thread expected");
  reader.caller=L;reader.bytes=luaL_checklstring(L,2,&reader.length);
  reader.offset=0;reader.visits=0;reader.block=(size_t)luaL_optinteger(L,5,3);
  if(!reader.block)return luaL_error(L,"nonzero reader block expected");
  before=lua_gettop(co);old=lua_status(co);
  status=lua_load(co,thread_reader,&reader,name);after=lua_gettop(co);
  lua_pushinteger(L,status);lua_pushinteger(L,before);lua_pushinteger(L,after);
  lua_pushinteger(L,old);lua_pushinteger(L,lua_status(co));lua_pushinteger(L,reader.visits);
  if(after>0){lua_pushvalue(co,-1);lua_xmove(co,L,1);}else lua_pushnil(L);
  lua_settop(co,before);
  return 7;
}

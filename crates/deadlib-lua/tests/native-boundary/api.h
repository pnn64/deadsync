/* Development-only Lua 5.1 ABI/error-boundary proof. */
#ifndef DEADLUA_BOUNDARY_API_H
#define DEADLUA_BOUNDARY_API_H
#include <stddef.h>
#ifdef BOUNDARY_BUILD
#define API __declspec(dllexport)
#else
#define API __declspec(dllimport)
#endif
typedef struct lua_State lua_State;
typedef int (*lua_CFunction)(lua_State *);
#define LUA_REGISTRYINDEX (-10000)
#define LUA_ENVIRONINDEX (-10001)
#define LUA_GLOBALSINDEX (-10002)
#define lua_upvalueindex(i) (LUA_GLOBALSINDEX-(i))
#define lua_pushcfunction(L,f) lua_pushcclosure(L,(f),0)
#define lua_newtable(L) lua_createtable(L,0,0)
typedef struct BoundaryValue {
  int integer;
  double number;
  const char *bytes;
  size_t length;
} BoundaryValue;
typedef int (*BoundaryDispatch)(void *, int, int, int, BoundaryValue *);
API lua_State *boundary_state_new(void);
API int boundary_state_free(lua_State *);
API size_t boundary_state_size(void);
API int boundary_invoke(lua_State *, void *, BoundaryDispatch, lua_CFunction);
API int lua_gettop(lua_State *);
API void lua_pushnumber(lua_State *, double);
API void lua_pushboolean(lua_State *, int);
API void lua_pushnil(lua_State *);
API void lua_pushlstring(lua_State *, const char *, size_t);
API void lua_pushvalue(lua_State *, int);
API double luaL_checknumber(lua_State *, int);
API int lua_error(lua_State *);
API void lua_call(lua_State *, int, int);
API int lua_checkstack(lua_State *, int);
API void lua_settop(lua_State *, int);
API void lua_replace(lua_State *, int);
API void lua_pushlightuserdata(lua_State *, void *);
API void *lua_touserdata(lua_State *, int);
API void lua_pushcclosure(lua_State *, lua_CFunction, int);
API void lua_createtable(lua_State *, int, int);
API void lua_gettable(lua_State *, int);
API void lua_settable(lua_State *, int);
API void lua_rawget(lua_State *, int);
API void lua_rawset(lua_State *, int);
API void lua_getfield(lua_State *, int, const char *);
API void lua_setfield(lua_State *, int, const char *);
API void lua_rawgeti(lua_State *, int, int);
API void lua_rawseti(lua_State *, int, int);
API int lua_getmetatable(lua_State *, int);
API int lua_setmetatable(lua_State *, int);
API int lua_next(lua_State *, int);
API int lua_type(lua_State *, int);
API int lua_rawequal(lua_State *, int, int);
API void lua_remove(lua_State *, int);
API void lua_insert(lua_State *, int);
API void *lua_newuserdata(lua_State *, size_t);
API size_t lua_objlen(lua_State *, int);
API const char *lua_tolstring(lua_State *, int, size_t *);
API void lua_getfenv(lua_State *, int);
API int lua_setfenv(lua_State *, int);
#endif

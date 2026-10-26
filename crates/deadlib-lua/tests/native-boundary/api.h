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
typedef struct BoundaryValue {
  int integer;
  double number;
  const char *bytes;
  size_t length;
} BoundaryValue;
typedef int (*BoundaryDispatch)(void *, int, int, int, BoundaryValue *);
API int boundary_invoke(void *, BoundaryDispatch, lua_CFunction);
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
#endif

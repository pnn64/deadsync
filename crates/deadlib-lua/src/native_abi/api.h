/* Lua 5.1 ABI declarations for the Rust runtime's C-only jump boundary. */
#ifndef DEADLUA_BOUNDARY_API_H
#define DEADLUA_BOUNDARY_API_H
#include <stddef.h>
#include <stdarg.h>
#include <stdio.h>
#if defined(_WIN32) && defined(BOUNDARY_BUILD)
#define API __declspec(dllexport)
#elif defined(_WIN32)
#define API __declspec(dllimport)
#else
#define API __attribute__((visibility("default")))
#endif
typedef struct lua_State lua_State;
typedef ptrdiff_t lua_Integer;
typedef int (*lua_CFunction)(lua_State *);
typedef const char *(*lua_Reader)(lua_State *, void *, size_t *);
typedef int (*lua_Writer)(lua_State *, const void *, size_t, void *);
typedef struct lua_Debug {
  int event;
  const char *name, *namewhat, *what, *source;
  int currentline, nups, linedefined, lastlinedefined;
  char short_src[60];
  int i_ci;
} lua_Debug;
typedef void (*lua_Hook)(lua_State *, lua_Debug *);
#define LUA_HOOKCALL 0
#define LUA_HOOKRET 1
#define LUA_HOOKLINE 2
#define LUA_HOOKCOUNT 3
#define LUA_HOOKTAILRET 4
#define LUA_MASKCALL (1 << LUA_HOOKCALL)
#define LUA_MASKRET (1 << LUA_HOOKRET)
#define LUA_MASKLINE (1 << LUA_HOOKLINE)
#define LUA_MASKCOUNT (1 << LUA_HOOKCOUNT)
typedef struct luaL_Reg { const char *name; lua_CFunction func; } luaL_Reg;
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
  lua_Integer wide_integer;
  lua_Reader reader;
  lua_Writer writer;
  void *userdata;
  lua_State *state;
} BoundaryValue;
typedef int (*BoundaryDispatch)(void *, int, int, int, BoundaryValue *);
API lua_State *boundary_state_new(void *, lua_State *);
API int boundary_state_free(lua_State *);
API size_t boundary_state_size(lua_State *);
API int boundary_invoke(lua_State *, void *, BoundaryDispatch, lua_CFunction);
API int boundary_hook(lua_State *, void *, BoundaryDispatch, lua_Hook, lua_Debug *);
API int boundary_read(lua_State *, void *, BoundaryDispatch, lua_Reader, void *, BoundaryValue *);
API int boundary_write(lua_State *, void *, BoundaryDispatch, lua_Writer, void *, const void *, size_t, BoundaryValue *);
API int lua_dump(lua_State *, lua_Writer, void *);
API lua_State *lua_tothread(lua_State *, int);
API lua_CFunction lua_tocfunction(lua_State *, int);
API lua_CFunction lua_atpanic(lua_State *, lua_CFunction);
API lua_State *lua_newthread(lua_State *);
API void lua_xmove(lua_State *, lua_State *, int);
API int lua_pushthread(lua_State *);
API int lua_status(lua_State *);
API int lua_resume(lua_State *, int);
API int lua_yield(lua_State *, int);
API void lua_setlevel(lua_State *, lua_State *);
API int boundary_call_level(lua_State *);
API int lua_getstack(lua_State *, int, lua_Debug *);
API int lua_sethook(lua_State *, lua_Hook, int, int);
API lua_Hook lua_gethook(lua_State *);
API int lua_gethookmask(lua_State *);
API int lua_gethookcount(lua_State *);
API int lua_getinfo(lua_State *, const char *, lua_Debug *);
API const char *lua_getlocal(lua_State *, const lua_Debug *, int);
API const char *lua_setlocal(lua_State *, const lua_Debug *, int);
API const void *lua_topointer(lua_State *, int);
API int lua_load(lua_State *, lua_Reader, void *, const char *);
API int luaL_loadbuffer(lua_State *, const char *, size_t, const char *);
API int luaL_loadstring(lua_State *, const char *);
API int luaL_loadfile(lua_State *, const char *);
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
API int lua_pcall(lua_State *, int, int, int);
API int lua_cpcall(lua_State *, lua_CFunction, void *);
API int lua_isnumber(lua_State *, int);
API int lua_isstring(lua_State *, int);
API int lua_isuserdata(lua_State *, int);
API int lua_iscfunction(lua_State *, int);
API double lua_tonumber(lua_State *, int);
API lua_Integer lua_tointeger(lua_State *, int);
API int lua_toboolean(lua_State *, int);
API void lua_pushinteger(lua_State *, lua_Integer);
API void lua_pushstring(lua_State *, const char *);
API const char *lua_typename(lua_State *, int);
API int lua_equal(lua_State *, int, int);
API int lua_lessthan(lua_State *, int, int);
API void lua_concat(lua_State *, int);
API const char *lua_getupvalue(lua_State *, int, int);
API const char *lua_setupvalue(lua_State *, int, int);
API int lua_gc(lua_State *, int, int);
#define LUA_GCSTOP 0
#define LUA_GCRESTART 1
#define LUA_GCCOLLECT 2
#define LUA_GCCOUNT 3
#define LUA_GCCOUNTB 4
#define LUA_GCSTEP 5
#define LUA_GCSETPAUSE 6
#define LUA_GCSETSTEPMUL 7
API const char *lua_pushvfstring(lua_State *, const char *, va_list);
API const char *lua_pushfstring(lua_State *, const char *, ...);
API int luaL_argerror(lua_State *, int, const char *);
API int luaL_typerror(lua_State *, int, const char *);
API void luaL_where(lua_State *, int);
API int luaL_error(lua_State *, const char *, ...);
API void luaL_checktype(lua_State *, int, int);
API void luaL_checkany(lua_State *, int);
API const char *luaL_checklstring(lua_State *, int, size_t *);
API const char *luaL_optlstring(lua_State *, int, const char *, size_t *);
API double luaL_optnumber(lua_State *, int, double);
API lua_Integer luaL_checkinteger(lua_State *, int);
API lua_Integer luaL_optinteger(lua_State *, int, lua_Integer);
API void luaL_checkstack(lua_State *, int, const char *);
API int luaL_checkoption(lua_State *, int, const char *, const char *const []);
API int luaL_newmetatable(lua_State *, const char *);
API void *luaL_checkudata(lua_State *, int, const char *);
API int luaL_getmetafield(lua_State *, int, const char *);
API int luaL_callmeta(lua_State *, int, const char *);
API const char *luaL_findtable(lua_State *, int, const char *, int);
API void luaL_register(lua_State *, const char *, const luaL_Reg *);
API void luaL_openlib(lua_State *, const char *, const luaL_Reg *, int);
#define luaI_openlib luaL_openlib
API int luaL_ref(lua_State *, int);
API void luaL_unref(lua_State *, int, int);

/* Lua 5.1 auxiliary buffer ABI, including caller-visible storage/macros. */
#define LUAL_BUFFERSIZE BUFSIZ
typedef struct luaL_Buffer {
  char *p;
  int lvl;
  lua_State *L;
  char buffer[LUAL_BUFFERSIZE];
} luaL_Buffer;
API void luaL_buffinit(lua_State *, luaL_Buffer *);
API char *luaL_prepbuffer(luaL_Buffer *);
API void luaL_addlstring(luaL_Buffer *, const char *, size_t);
API void luaL_addstring(luaL_Buffer *, const char *);
API void luaL_addvalue(luaL_Buffer *);
API void luaL_pushresult(luaL_Buffer *);
API const char *luaL_gsub(lua_State *, const char *, const char *, const char *);
API int luaopen_base(lua_State *);
API int luaopen_math(lua_State *);
API int luaopen_string(lua_State *);
API int luaopen_table(lua_State *);
API int luaopen_os(lua_State *);
API int luaopen_debug(lua_State *);
#define luaL_addchar(B,c) \
  ((void)((B)->p < (B)->buffer + LUAL_BUFFERSIZE || luaL_prepbuffer(B)), \
   (*(B)->p++ = (char)(c)))
#define luaL_putchar(B,c) luaL_addchar(B,c)
#define luaL_addsize(B,n) ((B)->p += (n))
#endif

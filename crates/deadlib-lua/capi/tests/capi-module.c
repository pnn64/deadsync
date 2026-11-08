#include "lua.h"
#include "lauxlib.h"
#ifdef _WIN32
#define EXPORT __declspec(dllexport)
#else
#define EXPORT __attribute__((visibility("default")))
#endif
static int sum(lua_State *L) {
  lua_pushnumber(L,luaL_checknumber(L,1)+luaL_checknumber(L,2));return 1;
}
static int fail(lua_State *L) {return luaL_error(L,"module %d/%s",17,"error");}
EXPORT int luaopen_capi_probe(lua_State *L) {
  static const luaL_Reg functions[]={{"sum",sum},{"fail",fail},{NULL,NULL}};
  luaL_register(L,"capi_probe",functions);return 1;
}

/* Library construction over the public C API; builtin execution stays in Rust. */
#include <math.h>

static const luaL_Reg open_base_functions[]={
  {"assert",builtin_body_0},
  {"collectgarbage",builtin_body_27},
  {"dofile",builtin_body_121},
  {"error",builtin_body_1},
  {"gcinfo",builtin_body_28},
  {"getfenv",builtin_body_22},
  {"getmetatable",builtin_body_13},
  {"loadfile",builtin_body_31},
  {"load",builtin_body_30},
  {"loadstring",builtin_body_24},
  {"next",builtin_body_9},
  {"pcall",builtin_body_15},
  {"print",builtin_body_18},
  {"rawequal",builtin_body_4},
  {"rawget",builtin_body_5},
  {"rawset",builtin_body_6},
  {"select",builtin_body_7},
  {"setfenv",builtin_body_23},
  {"setmetatable",builtin_body_14},
  {"tonumber",builtin_body_3},
  {"tostring",builtin_body_17},
  {"type",builtin_body_2},
  {"unpack",builtin_body_8},
  {"xpcall",builtin_body_16},
  {NULL,NULL}
};

static const luaL_Reg open_coroutine_functions[]={
  {"create",builtin_body_80},
  {"resume",builtin_body_81},
  {"running",builtin_body_82},
  {"status",builtin_body_83},
  {"wrap",builtin_body_84},
  {"yield",builtin_body_85},
  {NULL,NULL}
};

static const luaL_Reg open_math_functions[]={
  {"abs",builtin_body_32},
  {"acos",builtin_body_47},
  {"asin",builtin_body_46},
  {"atan2",builtin_body_52},
  {"atan",builtin_body_48},
  {"ceil",builtin_body_34},
  {"cosh",builtin_body_50},
  {"cos",builtin_body_37},
  {"deg",builtin_body_53},
  {"exp",builtin_body_39},
  {"floor",builtin_body_33},
  {"fmod",builtin_body_45},
  {"frexp",builtin_body_56},
  {"ldexp",builtin_body_57},
  {"log10",builtin_body_41},
  {"log",builtin_body_40},
  {"max",builtin_body_44},
  {"min",builtin_body_43},
  {"modf",builtin_body_55},
  {"pow",builtin_body_42},
  {"rad",builtin_body_54},
  {"random",builtin_body_58},
  {"randomseed",builtin_body_59},
  {"sinh",builtin_body_49},
  {"sin",builtin_body_36},
  {"sqrt",builtin_body_35},
  {"tanh",builtin_body_51},
  {"tan",builtin_body_38},
  {NULL,NULL}
};

static const luaL_Reg open_string_functions[]={
  {"byte",builtin_body_68},
  {"char",builtin_body_69},
  {"dump",builtin_body_77},
  {"find",builtin_body_72},
  {"format",builtin_body_76},
  {"gfind",builtin_body_78},
  {"gmatch",builtin_body_74},
  {"gsub",builtin_body_75},
  {"len",builtin_body_64},
  {"lower",builtin_body_70},
  {"match",builtin_body_73},
  {"rep",builtin_body_66},
  {"reverse",builtin_body_67},
  {"sub",builtin_body_65},
  {"upper",builtin_body_71},
  {NULL,NULL}
};

static const luaL_Reg open_table_functions[]={
  {"concat",builtin_body_96},
  {"foreach",builtin_body_97},
  {"foreachi",builtin_body_98},
  {"getn",builtin_body_99},
  {"maxn",builtin_body_100},
  {"insert",builtin_body_101},
  {"remove",builtin_body_102},
  {"setn",builtin_body_103},
  {"sort",builtin_body_104},
  {NULL,NULL}
};

static const luaL_Reg open_os_functions[]={
  {"clock",builtin_body_165},
  {"date",builtin_body_168},
  {"difftime",builtin_body_159},
  {"execute",builtin_body_163},
  {"exit",builtin_body_164},
  {"getenv",builtin_body_160},
  {"remove",builtin_body_161},
  {"rename",builtin_body_166},
  {"setlocale",builtin_body_169},
  {"time",builtin_body_167},
  {"tmpname",builtin_body_162},
  {NULL,NULL}
};

static const luaL_Reg open_debug_functions[]={
  {"debug",builtin_body_122},
  {"getfenv",builtin_body_25},
  {"gethook",builtin_body_118},
  {"getinfo",builtin_body_112},
  {"getlocal",builtin_body_115},
  {"getregistry",builtin_body_120},
  {"getmetatable",builtin_body_19},
  {"getupvalue",builtin_body_113},
  {"setfenv",builtin_body_26},
  {"sethook",builtin_body_119},
  {"setlocal",builtin_body_116},
  {"setmetatable",builtin_body_20},
  {"setupvalue",builtin_body_114},
  {"traceback",builtin_body_117},
  {NULL,NULL}
};

API int luaopen_base(lua_State *L) {
  lua_pushvalue(L,LUA_GLOBALSINDEX);lua_setfield(L,LUA_GLOBALSINDEX,"_G");
  luaL_register(L,"_G",open_base_functions);
  lua_pushstring(L,"Lua 5.1");lua_setfield(L,LUA_GLOBALSINDEX,"_VERSION");
  lua_pushcclosure(L,builtin_body_12,0);lua_pushcclosure(L,builtin_body_11,1);lua_setfield(L,-2,"ipairs");
  lua_pushcclosure(L,builtin_body_9,0);lua_pushcclosure(L,builtin_body_10,1);lua_setfield(L,-2,"pairs");
  lua_createtable(L,0,1);lua_pushvalue(L,-1);lua_setmetatable(L,-2);
  lua_pushstring(L,"kv");lua_setfield(L,-2,"__mode");
  lua_pushcclosure(L,builtin_body_29,1);lua_setfield(L,LUA_GLOBALSINDEX,"newproxy");
  luaL_register(L,"coroutine",open_coroutine_functions);return 2;
}
API int luaopen_math(lua_State *L) {
  luaL_register(L,"math",open_math_functions);
  lua_pushnumber(L,3.14159265358979323846);lua_setfield(L,-2,"pi");
  lua_pushnumber(L,HUGE_VAL);lua_setfield(L,-2,"huge");
  lua_getfield(L,-1,"fmod");lua_setfield(L,-2,"mod");return 1;
}
API int luaopen_string(lua_State *L) {
  luaL_register(L,"string",open_string_functions);
  lua_getfield(L,-1,"gmatch");lua_setfield(L,-2,"gfind");
  lua_createtable(L,0,1);lua_pushstring(L,"");lua_pushvalue(L,-2);lua_setmetatable(L,-2);
  lua_settop(L,-2);lua_pushvalue(L,-2);lua_setfield(L,-2,"__index");lua_settop(L,-2);return 1;
}
API int luaopen_table(lua_State *L) {luaL_register(L,"table",open_table_functions);return 1;}
API int luaopen_os(lua_State *L) {luaL_register(L,"os",open_os_functions);return 1;}
API int luaopen_debug(lua_State *L) {luaL_register(L,"debug",open_debug_functions);return 1;}

static const luaL_Reg open_io_functions[]={
  {"close",builtin_body_128},{"flush",builtin_body_129},
  {"input",builtin_body_130},{"lines",builtin_body_131},
  {"open",builtin_body_132},{"output",builtin_body_133},
  {"popen",builtin_body_134},{"read",builtin_body_135},
  {"tmpfile",builtin_body_136},{"type",builtin_body_137},
  {"write",builtin_body_138},{NULL,NULL}
};
static const luaL_Reg open_file_functions[]={
  {"close",builtin_body_128},{"flush",builtin_body_140},
  {"lines",builtin_body_141},{"read",builtin_body_142},
  {"seek",builtin_body_143},{"setvbuf",builtin_body_144},
  {"write",builtin_body_145},{"__gc",builtin_body_146},
  {"__tostring",builtin_body_147},{NULL,NULL}
};
static void open_io_environment(lua_State *L,lua_CFunction close) {
  lua_createtable(L,0,1);lua_pushcclosure(L,close,0);lua_setfield(L,-2,"__close");
}
static void open_io_standard(lua_State *L,FILE *file,int kind,int index,const char *name) {
  BoundaryValue value={0};
  FILE **pointer=(FILE **)lua_newuserdata(L,sizeof(FILE *));*pointer=NULL;
  lua_getfield(L,LUA_REGISTRYINDEX,"FILE*");lua_setmetatable(L,-2);
  *pointer=file;
  operation(L,75,kind,-1,&value);
  if(index>0){lua_pushvalue(L,-1);lua_rawseti(L,LUA_ENVIRONINDEX,index);}
  lua_pushvalue(L,-2);lua_setfenv(L,-2);lua_setfield(L,-3,name);
}
API int luaopen_io(lua_State *L) {
  luaL_newmetatable(L,"FILE*");lua_pushvalue(L,-1);lua_setfield(L,-2,"__index");
  luaL_register(L,NULL,open_file_functions);
  open_io_environment(L,builtin_body_148);lua_replace(L,LUA_ENVIRONINDEX);
  luaL_register(L,"io",open_io_functions);
  open_io_environment(L,builtin_body_149);
  open_io_standard(L,stdin,0,1,"stdin");
  open_io_standard(L,stdout,1,2,"stdout");
  open_io_standard(L,stderr,2,0,"stderr");
  lua_settop(L,-2);lua_getfield(L,-1,"popen");
  open_io_environment(L,builtin_body_150);lua_setfenv(L,-2);lua_settop(L,-2);return 1;
}

static const luaL_Reg open_package_functions[]={
  {"loadlib",builtin_body_158},{"seeall",builtin_body_153},{NULL,NULL}
};
static const luaL_Reg open_module_functions[]={
  {"module",builtin_body_152},{"require",builtin_body_151},{NULL,NULL}
};
static const lua_CFunction open_package_loaders[]={
  builtin_body_154,builtin_body_155,builtin_body_156,builtin_body_157
};
#ifdef _WIN32
#define OPEN_PACKAGE_PATH ".\\?.lua;!\\lua\\?.lua;!\\lua\\?\\init.lua;!\\?.lua;!\\?\\init.lua"
#define OPEN_PACKAGE_CPATH ".\\?.dll;!\\?.dll;!\\loadall.dll"
#define OPEN_PACKAGE_CONFIG "\\\n;\n?\n!\n-"
#else
#define OPEN_PACKAGE_PATH "./?.lua;/usr/local/share/lua/5.1/?.lua;/usr/local/share/lua/5.1/?/init.lua;/usr/local/lib/lua/5.1/?.lua;/usr/local/lib/lua/5.1/?/init.lua"
#define OPEN_PACKAGE_CPATH "./?.so;/usr/local/lib/lua/5.1/?.so;/usr/local/lib/lua/5.1/loadall.so"
#define OPEN_PACKAGE_CONFIG "/\n;\n?\n!\n-"
#endif
static void open_package_path(lua_State *L,const char *field,const char *variable,const char *fallback) {
  const char *path=getenv(variable);
  if(!path)lua_pushstring(L,fallback);
  else {
    path=luaL_gsub(L,path,";;",";\1;");
    luaL_gsub(L,path,"\1",fallback);lua_remove(L,-2);
  }
#ifdef _WIN32
  {
    char directory[MAX_PATH+1],*separator;
    DWORD size=sizeof(directory),length=GetModuleFileNameA(NULL,directory,size);
    if(!length||length==size||(separator=strrchr(directory,'\\'))==NULL)
      luaL_error(L,"unable to get ModuleFileName");
    else {
      *separator='\0';luaL_gsub(L,lua_tolstring(L,-1,NULL),"!",directory);lua_remove(L,-2);
    }
  }
#endif
  lua_setfield(L,-2,field);
}
API int luaopen_package(lua_State *L) {
  size_t i;
  luaL_newmetatable(L,"_LOADLIB");
  lua_pushcclosure(L,builtin_body_170,0);lua_setfield(L,-2,"__gc");
  luaL_register(L,"package",open_package_functions);
  lua_pushvalue(L,-1);lua_replace(L,LUA_ENVIRONINDEX);
  lua_createtable(L,4,0);
  for(i=0;i<sizeof(open_package_loaders)/sizeof(open_package_loaders[0]);i++){
    lua_pushcclosure(L,open_package_loaders[i],0);lua_rawseti(L,-2,(int)i+1);
  }
  lua_setfield(L,-2,"loaders");
  open_package_path(L,"path","LUA_PATH",OPEN_PACKAGE_PATH);
  open_package_path(L,"cpath","LUA_CPATH",OPEN_PACKAGE_CPATH);
  lua_pushstring(L,OPEN_PACKAGE_CONFIG);lua_setfield(L,-2,"config");
  luaL_findtable(L,LUA_REGISTRYINDEX,"_LOADED",2);lua_setfield(L,-2,"loaded");
  lua_createtable(L,0,0);lua_setfield(L,-2,"preload");
  lua_pushvalue(L,LUA_GLOBALSINDEX);luaL_register(L,NULL,open_module_functions);lua_settop(L,-2);
  return 1;
}

API void luaL_openlibs(lua_State *L) {
  static const luaL_Reg libraries[]={
    {"",luaopen_base},{"package",luaopen_package},{"table",luaopen_table},
    {"io",luaopen_io},{"os",luaopen_os},{"string",luaopen_string},
    {"math",luaopen_math},{"debug",luaopen_debug},{NULL,NULL}
  };
  const luaL_Reg *library;
  for(library=libraries;library->func;library++){
    lua_pushcclosure(L,library->func,0);lua_pushstring(L,library->name);lua_call(L,1,0);
  }
}
API int boundary_openlibs(lua_State *L) {luaL_openlibs(L);return 0;}

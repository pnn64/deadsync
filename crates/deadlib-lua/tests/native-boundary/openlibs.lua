-- luaL_openlibs must work on an empty state and retain the caller's C window.
local bare=_G==nil and io==nil and package==nil and assert==nil
local registry,count=library_registry()
local no_string_meta=library_metatable('')==nil
local n,s=library_all('argument',nil,17)
assert(bare and count==0 and no_string_meta and n==0)
assert(s.n==3 and s[1]=='argument'and s[2]==nil and s[3]==17)
local rows={}
local concat=table.concat
local function emit(label,...)
  local row={label}
  for i=1,select('#',...)do row[#row+1]=tostring(select(i,...))end
  rows[#rows+1]=concat(row,'\t')
end
emit('bare',bare,count==0,no_string_meta,n==0)
assert(debug.getfenv(library_all)==_G and _G==getfenv(0))
for _,name in ipairs{'_G','package','table','io','os','string','math','debug','coroutine'}do
  assert(type(_G[name])=='table'and registry._LOADED[name]==_G[name])
  emit('library.'..name,registry._LOADED[name]==_G[name])
end
for _,fn in ipairs{assert,table.concat,os.difftime,string.sub,math.abs,debug.getfenv,coroutine.resume}do
  assert(debug.getfenv(fn)==_G and debug.getinfo(fn).what=='C')
end
assert(debug.getfenv(require)==package and debug.getfenv(module)==package)
assert(debug.getfenv(package.loadlib)==_G and debug.getfenv(package.loaders[1])==package)
local env=debug.getfenv(io.read);local meta=registry['FILE*']
assert(env~=_G and env~=package and debug.getfenv(env.__close)==_G and debug.getfenv(meta.close)==_G)
assert(io.input()==io.stdin and io.output()==io.stdout and getmetatable(io.stdin)==meta)
assert(math.mod==math.fmod and string.gfind==string.gmatch and ('abc'):sub(2)=='bc')
emit('environments',debug.getfenv(library_all)==_G,debug.getfenv(require)==package,debug.getfenv(meta.close)==_G)
local f=assert(io.tmpfile());assert(f:write('openlibs\n')and f:seek('set')==0 and f:read('*l')=='openlibs'and f:close())
package.preload.openlibs_probe=function()return 42 end
assert(require('openlibs_probe')==42)
emit('bodies',io.type(f),require('openlibs_probe'))
-- Repeated full initialization reuses the tables, refreshes their members,
-- and does not adopt package/io environments in later initializers.
for round=1,4 do
  local libs={package=package,table=table,io=io,os=os,string=string,math=math,debug=debug,coroutine=coroutine}
  local old={assert=assert,read=io.read,stdin=io.stdin,preload=package.preload,proxy=newproxy(true)}
  local custom=round%2==0 and {marker=round}or _G
  debug.setfenv(library_all,custom)
  local n,s=library_all(round,nil,'tail')
  assert(n==0 and s.n==3 and s[1]==round and s[2]==nil and s[3]=='tail')
  assert(debug.getfenv(library_all)==custom and assert~=old.assert and io.read~=old.read and io.stdin~=old.stdin)
  for name,lib in pairs(libs)do assert(_G[name]==lib)end
  for _,fn in ipairs{assert,table.concat,os.difftime,string.sub,math.abs,debug.getfenv,coroutine.resume}do
    assert(debug.getfenv(fn)==custom)
  end
  assert(debug.getfenv(package.loadlib)==custom and debug.getfenv(require)==package)
  assert(debug.getfenv(meta.close)==custom and debug.getfenv(debug.getfenv(io.read).__close)==custom)
  assert(package.preload~=old.preload and package.loaded.openlibs_probe==42 and pcall(newproxy,old.proxy)==false)
  assert(io.type(old.stdin)=='file'and getmetatable(io.stdin)==meta)
  collectgarbage()
  emit('reopen.'..round,n,s.n,debug.getfenv(library_all)==custom,debug.getfenv(meta.close)==custom)
end
debug.setfenv(library_all,_G)
-- Registration failures stop at the failing library and retain earlier writes.
local old=registry._LOADED.table
for _,field in ipairs{'table','io','os','string','math','debug'}do
  local saved=registry._LOADED[field];local savedglobal=_G[field]
  registry._LOADED[field]=nil;_G[field]=17
  local ok,err=pcall(library_all)
  assert(not ok and err:find("name conflict for module '"..field.."'",1,true))
  assert(package.loaded==registry._LOADED and package.preload~=nil)
  emit('failure.'..field,ok,err:match("name conflict for module '[^']+'"))
  registry._LOADED[field]=saved;_G[field]=savedglobal
  library_all()
end
emit('complete',#rows)
return table.concat(rows,'\n')..'\n'

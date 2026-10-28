local m=native_files
local rows={}
local function emit(label,...)
  local row={label}
  for i=1,select('#',...)do row[#row+1]=tostring(select(i,...))end
  rows[#rows+1]=table.concat(row,'\t')
end
local function check_closed(f,label)
  assert(io.type(f)=='closed file'and tostring(f)=='file (closed)'and m.state(f)==false)
  for _,name in ipairs{'close','read','write','flush','seek','setvbuf','lines'}do
    local ok,err=pcall(f[name],f)
    assert(not ok and err:find('attempt to use a closed file',1,true),label..'.'..name..':'..tostring(err))
    emit(label..'.'..name,ok,err:match('attempt to use a closed file'))
  end
  assert(select('#',getmetatable(f).__gc(f))==0)
end
check_closed(m.closed(),'initial.closed')
for _,buffer in ipairs{'no','full','line'}do
  local f=m.new('alpha\nbeta\n17 2.5 gamma\n')
  assert(io.type(f)=='file'and tostring(f)==select(2,m.state(f)))
  assert(f:setvbuf(buffer,64))
  assert(f:read(2)=='al'and m.read(f,3)=='pha'and f:read('*l')=='')
  assert(m.read(f,1)=='b'and f:read('*l')=='eta')
  assert(f:read('*n')==17 and m.read(f,1)==' 'and f:read('*n')==2.5)
  assert(m.read(f,1)==' 'and f:read('*l')=='gamma'and f:read('*l')==nil)
  assert(f:seek('set')==0 and m.read(f,2)=='al'and f:read(0)==''and m.read(f,3)=='pha')
  assert(f:seek('end')and f:write('tail\n')and f:flush())
  assert(m.seek(f,0)==0 and f:read('*l')=='alpha')
  assert(f:seek('cur')and m.write(f,'X')and m.seek(f,-1,1)and f:read(1)=='X')
  assert(f:seek('set')==0)
  local iter=f:lines();assert(iter()=='alpha')
  assert(m.read(f,1)=='X'and iter()=='eta')
  local a,b=iter(),iter();assert(a=='17 2.5 gamma'and b=='tail'and iter()==nil)
  emit('mixed.'..buffer,io.type(f),a,b)
  assert(f:close()and m.state(f)==false)
  check_closed(f,'lua.closed.'..buffer)
end
-- The pointer payload remains authoritative when native code changes it.
do
  local f=m.new('old\n');assert(f:read(1)=='o')
  m.replace(f,'new\n');assert(f:read('*l')=='new')
  m.close(f);assert(io.type(f)=='closed file')
  m.replace(f,'again\n');assert(io.type(f)=='file'and f:read('*l')=='again')
  emit('pointer.replace',io.type(f),f:close())
end
do
  local f=m.new('transfer\n');assert(f:read(2)=='tr')
  local p=m.take(f);assert(io.type(f)=='closed file')
  local g=m.wrap(p);assert(io.type(g)=='file'and g:read('*l')=='ansfer')
  assert(g:close());emit('pointer.transfer',io.type(f),io.type(g))
end
-- A native close handler and GC see the same FILE** payload.
do
  local f=m.new('handler');local calls=0;local env={__close=m.close}
  debug.setfenv(f,env);assert(f:close()and io.type(f)=='closed file')
  local f=m.new('gc');debug.setfenv(f,{__close=m.close})
  getmetatable(f).__gc(f);assert(io.type(f)=='closed file')
  emit('close.handlers',io.type(f))
end
-- Numeric input uses the C image's actual scanner and preserves unread bytes.
for _,text in ipairs{'17x','2.5!','1e+z','0x1p2!','inf!','nan!','  -0x1.8p1?','\0x'}do
  local f=m.new(text);local n=f:read('*n');local tail=m.read(f,100)
  local value=n==nil and 'nil'or n~=n and 'nan'or n==0 and 1/n<0 and '-0'or tostring(n)
  emit('number',string.format('%q',text),value,string.format('%q',tail));assert(f:close())
end
do
  local f=m.open(native_files_source,'rb')
  local a,b,c=f:write('cannot-write')
  assert(a==nil and type(b)=='string'and type(c)=='number')
  emit('readonly.error',a,b,c)
  local iter=f:lines();local ok,err=pcall(iter)
  assert(not ok and type(err)=='string')
  emit('readonly.iterator',ok,err:match('Bad file descriptor')~=nil)
  assert(m.seek(f,0,1)>5)
  assert(f:seek('set')==0 and f:read(5)=='local'and f:close())
end
do
  local f=m.large()
  if f then
    local expected=m.seek(f,0,2);local actual=f:seek('end')
    assert(actual==expected,tostring(actual)..':'..tostring(expected))
    assert(f:seek('set',1)==1 and f:read(1)=='\0'and f:close())
    emit('large.position',actual)
  else emit('large.position','unavailable')end
end
for _,command in ipairs{'echo pipe-ok','exit 7'}do
  local f=m.pipe(command)
  local line=f:read('*l')
  assert(line==(command=='echo pipe-ok'and 'pipe-ok'or nil))
  assert(f:close()==true and m.state(f)==false)
  emit('pipe.close',command,line,io.type(f))
end
collectgarbage();collectgarbage()
native_file_keep=m.new('reachable during shutdown')
assert(io.type(native_file_keep)=='file')
emit('complete',#rows)
return table.concat(rows,'\n')..'\n'

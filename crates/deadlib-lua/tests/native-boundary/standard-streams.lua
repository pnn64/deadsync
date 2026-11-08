local m=native_files
local rows={}
local function hex(bytes)return(bytes:gsub('.',function(c)return string.format('%02x',c:byte())end))end
local function emit(label,...)
  local row={label}
  for i=1,select('#',...)do row[#row+1]=tostring(select(i,...))end
  rows[#rows+1]=table.concat(row,'\t')
end
for kind,name in ipairs{'stdin','stdout','stderr'}do
  assert(m.standard(io[name],kind-1)and io.type(io[name])=='file')
  assert(tostring(io[name])==select(2,m.state(io[name])))
  emit('pointer.'..name,m.standard(io[name],kind-1))
end
assert(m.standard_read(0,3)=='pha'and io.read('*l')=='')
assert(m.read(early_stdin,1)=='b'and io.read('*l')=='eta')
emit('stdin.mixed',true)
assert(m.reopen_io()==io and io.stdin~=early_stdin and io.read('*l')=='gamma')
emit('stdin.reopen',true)
local pointer=m.take(io.stdin)
assert(io.type(io.stdin)=='closed file'and io.type(early_stdin)=='file')
local chunk=assert(loadfile());assert(chunk()==17)
assert(m.standard_read(0,1)=='')
m.give(io.stdin,pointer);assert(io.type(io.stdin)=='file')
emit('stdin.kernel',chunk(),io.type(io.stdin),io.type(early_stdin))

for kind,name in ipairs{'stdout','stderr'}do
  for _,buffer in ipairs{'no','full','line'}do
    local path=os.tmpname();local file=io[name]
    m.capture_begin(kind,path)
    assert(file:setvbuf(buffer,64)and file:write('Lua:')and m.write(file,'C:'))
    assert(m.standard_write(kind,'global:'))
    local expected='Lua:C:global:'
    if kind==1 then
      local alternate=assert(io.tmpfile());io.output(alternate);assert(io.write('other'))
      print('print','line');io.output(io.stdout)
      assert(alternate:seek('set')==0 and alternate:read('*a')=='other'and alternate:close())
      expected=expected..'print\tline\n'
      local pointer=m.take(file);assert(io.type(file)=='closed file')
      print('independent');m.give(file,pointer);assert(io.type(file)=='file')
      expected=expected..'independent\n'
    else
      debug.debug();expected=expected..'lua_debug> '
    end
    local observer=assert(io.open(path,'rb'));local before=observer:read('*a');assert(observer:close())
    assert(file:write('end')and file:flush());expected=expected..'end'
    m.capture_end(kind)
    local reader=assert(io.open(path,'rb'));local actual=reader:read('*a');assert(reader:close()and os.remove(path))
    assert(actual==expected,name..'.'..buffer..':'..string.format('%q',actual)..':'..string.format('%q',expected))
    emit(name..'.'..buffer,hex(before),hex(actual))
  end
end
do
  local path=os.tmpname();local f=assert(io.open(path,'wb'));assert(f:write('unchanged')and f:close())
  m.capture_begin(1,path,'rb')
  local ok=pcall(print,'ignored output failure')
  local a,b,c=io.write('reported output failure')
  m.capture_end(1)
  local f=assert(io.open(path,'rb'));assert(f:read('*a')=='unchanged'and f:close()and os.remove(path))
  assert(ok and a==nil and type(b)=='string'and type(c)=='number')
  emit('stdout.error',ok,a,b,c)
end
for _,file in ipairs{io.stdin,io.stdout,io.stderr}do
  local ok,message=file:close();assert(ok==nil and message=='cannot close standard file'and io.type(file)=='file')
end
collectgarbage();collectgarbage()
emit('complete',#rows)
return table.concat(rows,'\n')..'\n'

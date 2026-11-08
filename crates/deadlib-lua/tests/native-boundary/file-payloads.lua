local m=native_files
local rows={}
local function emit(label,...)
  local row={label}
  for i=1,select('#',...)do row[#row+1]=tostring(select(i,...))end
  rows[#rows+1]=table.concat(row,'\t')
end
local function payload(f,label)
  assert(io.type(f)=='file'and m.state(f)==true,label)
  assert(tostring(f)==select(2,m.state(f)),label..'.pointer')
end
if early then
  payload(early,'early');assert(m.read(early,10)=='rly\n'and early:close())
  emit('before.C',io.type(early),m.state(early))
else emit('before.C','unavailable')end
for _,buffer in ipairs{'no','full','line'}do
  for _,factory in ipairs{'tmpfile','binary','text','input','output'}do
    local path=os.tmpname()
    local f
    if factory=='tmpfile'then f=assert(io.tmpfile())
    elseif factory=='binary'then f=assert(io.open(path,'w+b'))
    elseif factory=='text'then f=assert(io.open(path,'w+'))
    elseif factory=='input'then
      local w=assert(io.open(path,'wb'));assert(w:write('alpha\nbeta\n17 2.5 gamma\n')and w:close())
      f=io.input(path)
    else f=io.output(path)end
    payload(f,factory..'.'..buffer);assert(f:setvbuf(buffer,64))
    if factory~='input'then
      assert(f:write('alpha\nbeta\n17 2.5 gamma\n')and f:seek('set')==0)
    end
    if factory~='output'then
      assert(f:read(2)=='al'and m.read(f,3)=='pha'and f:read('*l')=='')
      assert(m.read(f,1)=='b'and f:read('*l')=='eta')
      assert(f:read('*n')==17 and m.read(f,1)==' 'and f:read('*n')==2.5)
      assert(m.read(f,1)==' 'and f:read('*l')=='gamma')
      emit('mixed.'..factory..'.'..buffer,io.type(f),f:read('*l'))
    else
      assert(m.write(f,'X')and f:write('Y')and f:flush())
      emit('mixed.'..factory..'.'..buffer,io.type(f))
    end
    if factory=='tmpfile'or factory=='binary'then
      assert(m.seek(f,0)==0 and m.read(f,1)=='a')
      local p=m.take(f);assert(io.type(f)=='closed file')
      local g=m.wrap(p);assert(g:read('*l')=='lpha'and g:close())
      emit('transfer.'..factory..'.'..buffer,io.type(f),io.type(g))
    elseif factory=='input'then
      assert(f:close()and m.state(f)==false)
    else
      assert(m.close(f)and io.type(f)=='closed file'and tostring(f)=='file (closed)')
    end
    if factory~='tmpfile'then assert(os.remove(path))else os.remove(path)end
    io.input(io.stdin);io.output(io.stdout)
  end
end
-- Anonymous files and named streams also expose pending buffers faithfully.
do
  local path=os.tmpname();local f=assert(io.open(path,'w+b'));assert(f:setvbuf('full',512))
  assert(f:write('pending')and m.state(f))
  local observer=assert(io.open(path,'rb'));assert(observer:read('*a')=='');assert(observer:close())
  assert(m.seek(f,0)==0 and m.read(f,7)=='pending'and f:close())
  assert(os.remove(path));emit('pending.write',true)
end
do
  local path=os.tmpname();local w=assert(io.open(path,'w'));assert(w:write('line1\nline2\n')and w:close())
  local iterator=io.lines(path)
  assert(iterator()=='line1'and iterator()=='line2'and iterator()==nil)
  assert(os.remove(path));emit('lines.close',true)
end
for _,command in ipairs{'echo from-lua','exit 7'}do
  local f=assert(io.popen(command,'r'));payload(f,'popen')
  local text=m.read(f,100)
  assert(text==(command=='echo from-lua'and 'from-lua\n'or ''))
  assert(f:close()==true and m.state(f)==false)
  emit('popen',command,io.type(f))
end
local failed=io.open('missing-payload-path/no-file','r');assert(failed==nil)
collectgarbage();collectgarbage()
local keep=assert(io.tmpfile());payload(keep,'shutdown');native_file_keep=keep
emit('complete',#rows)
return table.concat(rows,'\n')..'\n'

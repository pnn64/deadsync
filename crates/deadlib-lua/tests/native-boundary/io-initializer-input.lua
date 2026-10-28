-- The harness supplies alpha\nbeta\ngamma\n on the process's standard input.
library_all()
local rows={}
local function emit(label,value)rows[#rows+1]=label..'\t'..tostring(value)end
assert(io.read(2)=='al')
emit('input.prefix',true)
local old=io.stdin
library_open('io')
assert(io.stdin~=old and io.read('*l')=='pha'and old:read('*l')=='beta')
emit('input.shared',true)
old=nil;collectgarbage();collectgarbage()
library_open('io');collectgarbage();collectgarbage()
assert(io.read('*l')=='gamma'and io.read('*l')==nil)
emit('input.reopen',true)
return table.concat(rows,'\n')..'\n'

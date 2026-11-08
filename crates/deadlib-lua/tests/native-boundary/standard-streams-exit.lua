local m=native_files
assert(m.standard_mode(1,true)and io.stdout:setvbuf('full',256))
assert(io.stdout:write('Lua:')and m.standard_write(1,'C:'))
local pointer=m.take(io.stdout);assert(io.type(io.stdout)=='closed file')
print('print')
m.give(io.stdout,pointer)
os.exit(7)

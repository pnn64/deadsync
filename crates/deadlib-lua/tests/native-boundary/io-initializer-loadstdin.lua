-- The harness supplies return 17 on stdin, before I/O is initialized.
library_open('base');library_open('table')
local chunk=assert(loadfile());assert(chunk()==17)
library_open('io')
assert(io.read('*l')==nil and io.type(io.stdin)=='file')
return 'loadstdin.before_io\ttrue\n'

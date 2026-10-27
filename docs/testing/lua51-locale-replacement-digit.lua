assert(os.setlocale('ptb', 'ctype'))
local ok, error = pcall(string.gsub, 'x', 'x', '%' .. string.char(178))
print(ok, error)

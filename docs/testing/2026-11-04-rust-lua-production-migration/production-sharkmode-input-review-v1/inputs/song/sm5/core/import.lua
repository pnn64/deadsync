-- import things from _G, strip the <table> prefix for convenience
local function import(table)
	for i in pairs(_G[table]) do
		if not _ENV[i] then _ENV[i] = _G[table][i] end
	end
end

import'math'
import'string'
import'table'
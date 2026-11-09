-- mostly stolen from XeroOl's mirin template
-- https://github.com/XeroOl/notitg-mirin

-- takes Name= actors and places them in the _ENV table

return function()
	local function create(names, depth, t, actor)
		local name = names[depth]
		if depth < #names then
			if not t[name] then
				t[name] = {}
			end
			create(names, depth + 1, t[name], actor)
		else
			t[name] = actor
		end
		return
	end
	local function sweep(actor, skip)
		if actor.GetNumChildren then
			for i, child in pairs(actor:GetChildren()) do
				sweep(child)
			end
		end
		if skip then
			return
		end
		local name = actor:GetName()
		if name and name ~= '' then
			if find(name, '%.') or find(name, '%[') then
				local t = {}
				local temp = {}
				for n in gfind(name, '[^%.]+') do -- split names through `.`
					insert(temp, n)
				end
				
				-- iterate and find [] delimiters
				for i, v in ipairs(temp) do
					if find(v, '%[') then
						for k in gfind(v, '%[.+%]') do
							local index = gsub(k, '[^%w]', '')
							if not find(index, '%a') then
								index = _G.tonumber(index, 10)
							end
							insert(t, sub(v, 1, len(v) - len(k)))
							insert(t, index)
						end
					else
						insert(t, v)
					end
				end

				create(t, 1, _ENV, actor)
			else
				if _ENV[name] then
					error('Actor already exists with name ' .. name, 2)
				else
					_ENV[name] = actor
				end
			end
		end
	end

	sweep(self, true)
end
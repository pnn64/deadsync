local player
m = setmetatable({
	apply = function()
		for pn = 1, maximum_player_amount do
			-- apply any mods that have been collected, skip players that haven't gotten updates, asleep players should not be receiving updates by default
			if rawget(p, pn) and m.mods[pn] then
				po[pn]:FromString(concat(m.mods[pn], ','))
				m.mods[pn] = nil
			end
		end
	end, 
	mods = {},
},
{
	-- collect mods for the current frame, then apply at the end using m.apply()
	__call = function(m, mods)
		if player == nil then
			for pn = 1, maximum_player_amount do
				m[pn](mods)
			end
		else
			-- if a player is asleep, don't even bother collecting mods
			if not m.mods[player] then
				m.mods[player] = {mods}
			else
				insert(m.mods[player], mods)
			end
			player = nil
		end
	end,
	__index = function(m, pn)
		-- when m[pn] is called, set upvalue player to pn to signify a player specific call, then return m()
		player = pn
		return m
	end
})
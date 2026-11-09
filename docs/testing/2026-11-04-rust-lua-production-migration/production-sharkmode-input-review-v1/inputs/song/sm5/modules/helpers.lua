-- the multi tool perframe
function range(begin, len, sustain)
	if beat < begin then
	elseif beat < begin + len then a = (beat - begin) / len return a
	elseif beat < begin + len + (sustain or 0.1) then a = 1 return 1
	end
end


do
	local td = song:GetTimingData()
	local bpms = td:GetBPMsAndTimes(true)
	local cache = {}
	-- second based range
	function srange(begin, len, sustain)
		sustain = sustain or 0.1
		if not cache[begin] then cache[begin] = 1 / (td:GetBPMAtBeat(td:GetBeatFromElapsedTime(begin - globaloffset)) / 60) end
		--[[
			while we're using seconds, being able to supplement the length and sustain by beats makes this a lot more streamlined,
			but we need to know the bpm at the current beat rather than relying on a crude sp:GetCurBPS() call
			since it will reflect on ranges fired in the past,
			so get the current bpm, cache it, and convert it into how long a beat is in seconds at that bpm
		]]
		len = len * cache[begin]
		sustain = sustain * cache[begin]
		if second < begin then
		elseif second < begin + len then
			a = (second - begin) / len
			return a
		elseif second < begin + len + sustain then
			a = 1
			return 1
		end
	end

	-- second based range, with second based len / sustain (true time)
	function srangett(begin, len, sustain)
		sustain = sustain or 0.1
		if second < begin then
		elseif second < begin + len then
			a = (second - begin) / len
			return a
		elseif second < begin + len + sustain then
			a = 1
			return 1
		end
	end
end

-- 60fps (ish) capped perframe
do
	local capped_beat, last_frame = -999, -999
	function range_capped(begin, len, sustain)
		if frame ~= last_frame then capped_beat = beat end
		if beat < begin then
		elseif beat < begin + len then a = (capped_beat - begin) / len return a
		elseif beat < begin + len + (sustain or 0.1) then a = 1 return 1
		end
	end
end

-- column specific and player specific helper handler
-- todo plr specific shortcut where plr.mod = plr.mod + x -> for pn = 1, 2 do plr[pn].mod = plr[pn].mod + x end
do
	local i = _G.FUCK_EXE and 1 or 0 -- columns start at 0 unless otherwise specified
	local player
	local _specific = {
		col = setmetatable(
			{},
			{
				__call = function(t, mod)
					for c = 1, 4 do
						m('*9999999 ' .. col[mod][c] .. ' ' .. mod .. c - i)
					end
					return t
				end,
				__index = function(t, pn)
					player = pn
					return t.plr.col
				end
			}
		),
		plr = setmetatable(
			{
				col = setmetatable(
					{},
					{
						__call = function(t, mod)
							if player == nil then
								for pn = 1, maximum_player_amount do
									for c = 1, 4 do
										m[pn]('*9999999 ' .. plr[pn].col[mod][c] .. ' ' .. mod .. c - i)
									end
								end
							else
								for c = 1, 4 do
									m[player]('*9999999 ' .. plr[player].col[mod][c] .. ' ' .. mod .. c - i)
								end
							end
							return t
						end,
						__index = function(t, pn)
							player = pn
							return t
						end 
					}
				)
			},
			{
				__call = function(t, mod)
					if player == nil then
						for i = 1, maximum_player_amount do
							m[i]('*9999999 ' .. plr[i][mod] .. ' ' .. mod)
						end
					else
						m[player]('*9999999 ' .. plr[player][mod] .. ' ' .. mod)
					end
					return t
				end,
				__index = function(t, pn)
					player = pn
					return t
				end
			}
		)
	}

	local _reset = {
		col = setmetatable(
			{},
			{
				__call = function(t, mod)
					for c = 1, 4 do
						col[mod][c] = 0
					end
					return t
				end,
				__index = function(t, pn)
					player = pn
					return t.plr.col
				end
			}
		),
		plr = setmetatable(
			{
				col = setmetatable(
					{},
					{
						__call = function(t, mod)
							if player == nil then
								for i = 1, maximum_player_amount do
									for c = 1, 4 do
										plr[i].col[mod][c] = 0
									end
								end
							else
								for c = 1, 4 do
									plr[player].col[mod][c] = 0
								end
							end
							return t
						end,
						__index = function(t, pn)
							player = pn
							return t
						end 
					}
				)
			},
			{
				__call = function(t, mod)
					if player == nil then
						for i = 1, maximum_player_amount do
							plr[i][mod] = 0
						end
					else
						plr[player][mod] = 0
					end
					return t
				end,
				__index = function(t, pn)
					player = pn
					return t
				end
			}
		)
	}

	-- huuUuuuuuUuUUuUoOOOOOœuGH
	_specific.col.plr = _specific.plr.col
	_reset.col.plr = _reset.plr.col

	-- dummy tables so player value is reset only when the function is initially called, and not on repeat calls
	specific = setmetatable({},
		{
			__index = function(t, k)
				player = nil
				return _specific[k]
			end
		}
	)
	
	reset = setmetatable({},
		{
			__index = function(t, k)
				player = nil
				return _reset[k]
			end
		}
	)
end

-- helpers for additive easing column and player specific mods
do
	local temp = {}
	local function handle_numbers(lhs, rhs)
		-- flip positions if lhs isn't a table so rhs is ALWAYS the non table value if case be
		if type(lhs) ~= 'table' then lhs, rhs = rhs, lhs end
		-- convert any numbers into a table the size of lhs, containing the same number
		if type(rhs) == 'number' then
			for i = 1, #lhs do temp[i] = rhs end
			return lhs, temp
		end
		return lhs, rhs
	end

	-- metatables to dictate value assignment for math on player and column specific variables
	-- notes: this is a very heavy operation as values aren't cached and that passing tables in function pass by reference, requiring the creation of a new table for *every call* onto the table
	local mt = {
		__add = function(lhs, rhs)
			-- check for values, flip if necessary
			lhs, rhs = handle_numbers(lhs, rhs)
			-- create temporary table to store new values, assign the lhs metatable to it as the returning table should also be able to be mathed on
			local temp = setmetatable({}, getmetatable(lhs))
			-- combine values from lhs and rhs into temp
			for i = 1, #lhs do temp[i] = lhs[i] + rhs[i] end
			-- assign temp as the new value
			-- todo: figure out variable value returns without but only replace the data of the assigning table with contents of the calulated values, and not the table itself
			return temp
		end,
		-- rinse and repeat for other arrangments
		__sub = function(lhs, rhs)
			lhs, rhs = handle_numbers(lhs, rhs)
			local temp = setmetatable({}, getmetatable(lhs))
			for i = 1, #lhs do temp[i] = lhs[i] - rhs[i] end
			return temp
		end,
		__mul = function(lhs, rhs)
			lhs, rhs = handle_numbers(lhs, rhs)
			local temp = setmetatable({}, getmetatable(lhs))
			for i = 1, #lhs do temp[i] = lhs[i] * rhs[i] end
			return temp
		end,
		__div = function(lhs, rhs)
			lhs, rhs = handle_numbers(lhs, rhs)
			local temp = setmetatable({}, getmetatable(lhs))
			for i = 1, #lhs do temp[i] = lhs[i] / rhs[i] end
			return temp
		end,
		__unm = function(t)
			local temp = setmetatable({}, getmetatable(lhs))
			for i = 1, #t do temp[i] = -t[i] end
			return temp
		end,
		__index = function() return 0 end
	}

	col = setmetatable({}, {
		__index = function(t, k)
			t[k] = setmetatable({0, 0, 0, 0}, mt)
			return t[k]
		end
	})
	plr = setmetatable(
		{},
		{__index = function(t, k)
			print('__index', t, k, t[1][k])
			t[k] = {}
			for pn = 1, maximum_player_amount do
				t[k][pn] = t[pn][k]
			end
			setmetatable(t[k], mt)
			return t[k]
		end}
	)
	for pn = 1, maximum_player_amount do
		plr[pn] = setmetatable(
			{
				col = setmetatable({}, {
					__index = function(t, k)
						t[k] = setmetatable({0, 0, 0, 0}, mt)
						return t[k]
					end
				})
			},
			mt
		)
	end
end

-- catcher to ensure specific funcs are run only once in the update loop
runonce = setmetatable(
	{},
	{
		__index = function(t, n)
			if not rawget(t, n) then
				t[n] = false
				return true
			end
			return false
		end
	}
)

-- transforms a hardcoded notedata to player notedata if a note exists at the point of the original notedata
function actualnd(nd)
	local p = p[1] or p[2]
	local hit = -1
	for i, v in ipairs(nd) do
		local note = p:GetNoteData(v[1] - 0.1, v[1] + 0.1)
		if #note > 0 then
			if type(note[1][3]) ~= 'number' then remove(note, 1) end
			if hit == note[1][1] then remove(note, 1) end
			if nd[i].length then note[1].length = nd[i].length end
			nd[i] = _G.unpack(note)
			hit = nd[i][1]
		end
	end
	return nd
end

function avnd2nitgnd(nd)
	for i, note in ipairs(nd) do
		insert(note, 1)
	end
	return nd
end

function normalisecolumns(nd)
	for i, note in ipairs(nd) do
		nd[i][2] = nd[i][2] + 1
	end
	return nd
end
-- notedata = {}

-- extra math things
-- about 100% of the time, floored div is wanted over truncted div
function mod(a, b) return a - floor(a / b) * b end

-- pseudo seeded random
function rand(a) return mod(sin(a * a * 99999) * 99 + 108 , 1) end
function round(num, ndp) local mult = 10 ^ (ndp or 0) return floor(num * mult + 0.5) / mult end

function lerp(v0, v1, t) return v0 + t * (v1 - v0) end
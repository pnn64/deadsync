local amod = PlayerOptions.AMod
local camod = PlayerOptions.CAMod
local avmod = PlayerOptions.AVMod
local t = {}
t.speed = {{}, {}}

local bpm = 120
function t.setbpm(b)
	bpm = b
end
function t.grab(func)
	for pn = 1, 2 do
		if rawget(p, pn) then
			if (po[pn]:MMod()) then
				t.speed[pn][1] = (po[pn]:MMod())
				t.speed[pn].type = 'm'
				t.speed[pn].func = 'MMod'
			elseif (po[pn]:CMod()) then
				t.speed[pn][1] = (po[pn]:CMod())
				t.speed[pn].type = 'c'
				t.speed[pn].func = 'CMod'
			-- speedmods exclusive to outfox in the current moment
			-- average
			elseif amod and (po[pn]:AMod()) then
				t.speed[pn][1] = (po[pn]:AMod())
				t.speed[pn].type = 'a'
				t.speed[pn].func = 'AMod'
			-- constant average
			elseif camod and (po[pn]:CAMod()) then
				t.speed[pn][1] = (po[pn]:CAMod())
				t.speed[pn].type = 'ca'
				t.speed[pn].func = 'CAMod'
			-- auto velocity
			elseif avmod and (po[pn]:AVMod()) then
				t.speed[pn][1] = (po[pn]:AVMod())
				t.speed[pn].type = 'av'
				t.speed[pn].func = 'AVMod'
			-- and lastly XMod, to ensure we don't erronously capture it while using another speedmod
			elseif (po[pn]:XMod()) then
				t.speed[pn][1] = (po[pn]:XMod()) * bpm
				t.speed[pn].type = 'x'
				t.speed[pn].func = 'MMod'
			else -- some different builds introduced additional speedmod types, accounting for every single one that will spawn in the future is impossible, so fall back to 2 in these cases
				t.speed[pn][1] = bpm * 2
				t.speed[pn].type = 'x'
				t.speed[pn].func = 'MMod'
			end
			if func then func(pn) end
		end
	end
end

return t
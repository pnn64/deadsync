-- Keep score refreshes separate from authored BitmapText tweens. Multiple
-- judgments before the next underlay update need only one refresh per player.
function CreateScoreRedrawQueue(players)
	local pending = {}
	local function request(player, actor)
		pending[player] = actor
	end
	local function redraw()
		for player in ivalues(players) do
			local score = pending[player]
			pending[player] = nil
			if score then score:playcommand("RedrawScore") end
		end
	end
	return request, redraw
end

return function(player)
	local po = GAMESTATE:GetPlayerState(player):GetPlayerOptions('ModsLevel_Preferred')

	local shuffle = po:Shuffle() or po:SoftShuffle() or po:SuperShuffle()
	local notes_inserted = (po:Wide() or po:Skippy() or po:Quick() or po:Echo() or
													po:BMRize() or po:Stomp() or po:Big())
	local notes_removed = (po:Little()  or po:NoHolds() or po:NoStretch() or
													po:NoHands() or po:NoJumps() or po:NoFakes() or
													po:NoLifts() or po:NoQuads() or po:NoRolls())

	-- If shuffle is used or notes were inserted/removed, we can't compute it
	-- return early
	-- TODO(teejusb): Add support for Backwards()
	if shuffle or notes_inserted or notes_removed or po:Backwards() then
		return nil
	end

	local flip = po:Flip() > 0
	local invert = po:Invert() > 0
	local left = po:Left()
	local right = po:Right()
	local mirror = po:Mirror()
	local udmirror = po:UDMirror()
	local lrmirror = po:LRMirror()

	-- Combining flip and invert results in unusual spacing so ignore it.
	if flip and invert then
		return nil
	end

	local has_turn = flip or invert or left or right or mirror
	local style = GAMESTATE:GetCurrentStyle()
	local num_columns = style:ColumnsPerPlayer()

	-- We only resolve turn mods in 4 and 8 panel.
	if num_columns ~= 4 and num_columns ~= 8 then
		if not has_turn then
			-- Not turn mod used, return 1-to-1 mapping.
			return range(num_columns)
		else
			-- If we are using turn mods in modes without 4 or 8 columns then return
			-- early since we don't try to resolve them.
			return nil
		end
	end

	local column_mapping = {1, 2, 3, 4}

	if flip then
		column_mapping = {column_mapping[4], column_mapping[3], column_mapping[2], column_mapping[1]}
	end

	if invert then
		column_mapping = {column_mapping[2], column_mapping[1], column_mapping[4], column_mapping[3]}
	end

	if left then
		column_mapping = {column_mapping[2], column_mapping[4], column_mapping[1], column_mapping[3]}
	end

	if right then
		column_mapping = {column_mapping[3], column_mapping[1], column_mapping[4], column_mapping[2]}
	end

	if mirror then
		column_mapping = {column_mapping[4], column_mapping[3], column_mapping[2], column_mapping[1]}
	end

	if udmirror then
		column_mapping = {column_mapping[1], column_mapping[3], column_mapping[2], column_mapping[4]}
	end

	if lrmirror then
		column_mapping = {column_mapping[4], column_mapping[2], column_mapping[3], column_mapping[1]}
	end

	if num_columns == 8 then
		for i=1,4 do
			column_mapping[4+i] = column_mapping[i] + 4
		end

		-- Flip, Mirror. and LRMirror all swap left and right sides.
		-- If an odd number of them are set then swap.
		local swapCount = 0
		if flip then swapCount = swapCount + 1 end
		if mirror then swapCount = swapCount + 1 end
		if lrmirror then swapCount = swapCount + 1 end

		if swapCount % 2 == 1 then
			for i=1,4 do
				column_mapping[i] = column_mapping[i] + 4
				column_mapping[i+4] = column_mapping[i+4] - 4
			end
		end
	end

	return column_mapping
end
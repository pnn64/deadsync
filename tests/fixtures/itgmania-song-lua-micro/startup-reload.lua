HARNESS_NATIVE_RELOAD_COUNT = (HARNESS_NATIVE_RELOAD_COUNT or 0) + 1
local count = HARNESS_NATIVE_RELOAD_COUNT
local selected = GAMESTATE:GetCurrentSteps(PLAYER_1)
if count == 1 then
	for _, steps in ipairs(GAMESTATE:GetCurrentSong():GetStepsByStepsType("StepsType_Dance_Single")) do
		if steps:GetDifficulty() == "Difficulty_Easy" then
			GAMESTATE:SetCurrentSteps(PLAYER_1, steps)
		end
	end
end

-- Keep the returned constructor: native Lua tail calls can lose this chunk's
-- source frame, while these command functions retain their source metadata.
return Def.ActorFrame {
	Name = "NativeReloadControl",
	OnCommand = function(self)
		local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
		MESSAGEMAN:Broadcast("HarnessReloadControl", {
			count = count,
			difficulty = selected:GetDifficulty(),
			possible = stats:GetPossibleDancePoints(),
			current_possible = stats:GetCurrentPossibleDancePoints(),
			top_name = SCREENMAN:GetTopScreen():GetName(),
		})
		if count == 1 then SCREENMAN:SetNewScreen("ScreenGameplay") end
	end,
}

local t = {}
local multipliers = {
    ['Difficulty_Beginner'] = 0.35,
	['Difficulty_Easy'] = 0.5,
	['Difficulty_Medium'] = 0.75,
	['Difficulty_Hard'] = 1,
	['Difficulty_Challenge'] = 1,
	['Difficulty_Edit'] = 1,
}
local currentmultiplier = {1, 1}
local mastermultiplier = 1

t.updatemultipliers = function()
    for pn = 1, 2 do
        local steps = GAMESTATE:GetCurrentSteps(pn - 1)
        if steps then
            currentmultiplier[pn] = multipliers[steps:GetDifficulty()]
        end
    end

    mastermultiplier = min(currentmultiplier[1], currentmultiplier[2])
end

t.getmult = function(pn) return currentmultiplier[pn] end
t.getmastermult = function() return mastermultiplier end

return t
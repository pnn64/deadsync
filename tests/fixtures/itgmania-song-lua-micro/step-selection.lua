local all = GAMESTATE:GetCurrentSong():GetAllSteps()
assert(#all == 6)
local shared = GAMESTATE:GetCurrentStyle():GetStyleType() == 'StyleType_TwoPlayersSharedSides'
local function expect(first, second)
    assert(GAMESTATE:GetCurrentSteps(0) == (first and all[first] or nil))
    assert(GAMESTATE:GetCurrentSteps(1) == (second and all[second] or nil))
end
assert(GAMESTATE:SetCurrentSteps(0, all[3]) == GAMESTATE)
assert(GAMESTATE:SetCurrentSteps(1, all[3]) == GAMESTATE)
local hits = {0, 0}
local reenter = false
local function snapshot(label)
    local first, second = GAMESTATE:GetCurrentSteps(0), GAMESTATE:GetCurrentSteps(1)
    MESSAGEMAN:Broadcast(table.concat({label,
        first and first:GetDescription() or 'nil', second and second:GetDescription() or 'nil',
        tostring(hits[1]), tostring(hits[2])}, ':'))
end
return Def.ActorFrame{
    CurrentStepsP1ChangedMessageCommand=function()
        hits[1] = hits[1] + 1
        if reenter then
            reenter = false
            GAMESTATE:SetCurrentSteps(0, all[2])
        end
    end,
    CurrentStepsP2ChangedMessageCommand=function() hits[2] = hits[2] + 1 end,
    OnCommand=function()
        assert(GAMESTATE:SetCurrentSteps(0, all[1]) == GAMESTATE)
        expect(1, 1)
        assert(GAMESTATE:GetEasiestStepsDifficulty() == 'Difficulty_Easy')
        snapshot('different-music')
        GAMESTATE:SetCurrentSteps(1, all[2])
        expect(shared and 2 or 1, 2)
        snapshot('same-music')
        GAMESTATE:SetCurrentSteps(0, GAMESTATE:GetCurrentSteps(0))
        snapshot('same-pointer')
        assert(not pcall(function() GAMESTATE:SetCurrentSteps(2, all[1]) end))
        assert(not pcall(function() GAMESTATE:SetCurrentSteps(1.5, all[1]) end))
        assert(not pcall(function() GAMESTATE:GetCurrentSteps(1.5) end))
        assert(not pcall(function() GAMESTATE:SetCurrentSteps(0, {}) end))
        assert(GAMESTATE:GetCurrentSteps('p1') == GAMESTATE:GetCurrentSteps(0))
        if not shared then
            assert(not pcall(function() GAMESTATE:SetCurrentSteps(0, all[4]) end))
            expect(1, 2)
        end
        assert(GAMESTATE:SetCurrentSteps(0, nil) == GAMESTATE)
        expect(nil, 2)
        snapshot('clear-one')
        GAMESTATE:SetCurrentSteps(1, all[2])
        expect(2, 2)
        snapshot('fill-empty')
        GAMESTATE:SetCurrentSteps(0, all[5])
        expect(5, 5)
        snapshot('inherited-music')
        GAMESTATE:SetCurrentSteps(1, all[6])
        expect(6, 6)
        snapshot('authored-spelling')
        reenter = true
        GAMESTATE:SetCurrentSteps(0, all[1])
        expect(2, 2)
        snapshot('reentrant-selection')
    end,
}

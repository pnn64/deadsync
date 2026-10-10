local states = {
    GAMESTATE:GetPlayerState(PLAYER_1),
    GAMESTATE:GetPlayerState(PLAYER_2),
}
local options = {
    states[1]:GetPlayerOptions("ModsLevel_Song"),
    states[2]:GetPlayerOptions("ModsLevel_Song"),
}
local function close(actual, expected)
    assert(math.abs(actual - expected) < 0.0001)
end
local function check(po, phase)
    if phase == 0 then
        close(po:ScrollSpeed(), 1)
        close(po:ScrollBPM(), 200)
        close(po:TimeSpacing(), 0)
        close(po:MaxScrollBPM(), 0)
        po:MMod(200, 7)
        close(po:MaxScrollBPM(), 200)
        local previous, speed = po:ScrollSpeed(4, 2)
        close(previous, 1)
        close(speed, 7)
        close(po:XMod(), 4)
        close(po:MMod(), 200)
        local amount, approach = po:MaxScrollBPM(0, 3)
        close(amount, 200)
        close(approach, 7)
        assert(po:MMod() == nil)
        close(po:XMod(), 4)
        close(select(2, po:XMod()), 2)
        close(select(2, po:ScrollBPM()), 7)
    elseif phase == 1 then
        po:ScrollBPM(360, 5)
        po:TimeSpacing(1, 6)
        close(po:CMod(), 360)
        assert(po:XMod() == nil and po:MMod() == nil)
        close(po:ScrollSpeed(), 4)
        po:ScrollSpeed(-0.5, 0)
        close(po:ScrollSpeed(), -0.5)
        close(po:CMod(), 360)
        po:TimeSpacing(0)
        close(po:XMod(), -0.5)
        assert(po:CMod() == nil)
        assert(po:ScrollBPM(nil, nil, true) == po)
    elseif phase == 2 then
        assert(not pcall(function() po:ScrollSpeed(2, -1) end))
        close(po:XMod(), 2)
        close(select(2, po:ScrollSpeed()), 0)
        po:CMod(480, 9)
        close(po:ScrollSpeed(), 1)
        close(po:TimeSpacing(), 1)
        close(po:MaxScrollBPM(), 0)
        close(po:ScrollBPM(), 480)
        close(select(2, po:ScrollSpeed()), 9)
        close(select(2, po:TimeSpacing()), 9)
        close(select(2, po:MaxScrollBPM()), 9)
    elseif phase == 3 then
        po:FromString("clearall")
        close(po:ScrollSpeed(), 1)
        close(po:ScrollBPM(), 200)
        close(po:TimeSpacing(), 0)
        close(po:MaxScrollBPM(), 0)
        close(po:XMod(), 1)
    end
end
return Def.ActorFrame{
    OnCommand = function(self)
        for _, po in ipairs(options) do check(po, 0) end
        local phase = 0
        self:SetUpdateFunction(function()
            local next_phase = math.floor(GAMESTATE:GetSongBeat())
            if next_phase == phase then return end
            phase = next_phase
            for _, po in ipairs(options) do check(po, phase) end
        end)
    end,
}

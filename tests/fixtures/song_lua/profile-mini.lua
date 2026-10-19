-- The reverse path calculation from 7th Gear, reduced to one column.
-- Read the saved options before the song's actors are constructed.
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local options = player:GetPlayerOptions("ModsLevel_Song")
local mini = options:Mini()
local scale = 1 - mini / 2
local speed = options:MMod() / 124

return Def.ActorFrame{
    InitCommand=function(self)
        for _, level in ipairs({"Preferred", "Stage", "Song", "Current"}) do
            assert(player:GetPlayerOptions("ModsLevel_" .. level):Mini() == mini)
        end
        local handler = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            :GetChild("NoteField"):GetColumnActors()[1]:GetPosHandler()
        handler:SetSplineMode("NoteColumnSplineMode_Position")
        handler:SetBeatsPerT(0.125)
        local spline = handler:GetSpline()
        local size = math.round(7.5 * 8 / (speed * scale))
        spline:SetSize(size)
        for point = 1, size do
            spline:SetPoint(point, {-96, 135 / scale - 8 * (point - 1) * speed, 0})
        end
        spline:Solve()
        -- 7th Gear restores its saved Mini through this Song-level setter.
        -- Playback must assign that value, not add the profile a second time.
        options:Mini(mini, 10000)
    end,
    Def.Quad{
        Name="InitialMini",
        InitCommand=function(self) self:x(mini) end,
    },
}

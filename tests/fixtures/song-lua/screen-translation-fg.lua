return Def.ActorFrame{
    OnCommand=function(self)
        local screen=SCREENMAN:GetTopScreen()
        screen:xy(32,48)
        local moved=false
        self:SetUpdateFunction(function()
            if not moved and GAMESTATE:GetSongBeat() >= 1 then
                moved=true
                screen:linear(0.5):y(240)
            end
        end)
    end,
    Def.Quad{
        Name="ForegroundFlat",
        OnCommand=function(self) self:xy(100,100):zoomto(64,64) end,
    },
    Def.ActorFrame{
        OnCommand=function(self) self:fov(80) end,
        Def.Quad{
            Name="ForegroundDepth",
            OnCommand=function(self) self:xy(200,200):z(-100):zoomto(64,64) end,
        },
    },
}

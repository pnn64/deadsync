local helper, phase = nil, 0
return Def.ActorFrame{
    Name = "Root",
    InitCommand = function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            if phase == 0 and beat >= 1 then
                helper:linear(0.5):y(150):linear(0.5):y(0)
                SCREENMAN:GetTopScreen():vibrate():effectmagnitude(20, 20, 0)
                phase = 1
            elseif phase == 1 and beat >= 1.98 then
                helper:finishtweening():linear(0.5):z(200):linear(0.5):z(0)
                SCREENMAN:GetTopScreen():effectmagnitude(0, 0, 0)
                phase = 2
            end
            for pn = 0, 1 do
                GAMESTATE:GetPlayerState(pn):GetPlayerOptions("ModsLevel_Song"):Tipsy(helper:GetY() / 100, 10000)
            end
        end)
    end,
    Def.Quad{
        Name = "Helper",
        InitCommand = function(self) helper = self; self:visible(false) end,
    },
    Def.ActorFrame{
        Name = "Base",
        InitCommand = function(self) self:xy(100, 120):basezoomx(0.75):basezoomy(0.5) end,
        Def.Sprite{
            Name = "Sprite", Texture = "fit-rect.png",
            InitCommand = function(self) self:basezoom(0.8):zoomx(1.5):zoomy(2) end,
        },
    },
}

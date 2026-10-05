local target, shadow
local n = 0
return Def.ActorFrame{
    OnCommand=function(self)
        local started=false
        self:SetUpdateFunction(function()
            if not started and GAMESTATE:GetSongBeat() >= 1 then
                started=true
                MESSAGEMAN:Broadcast("Show")
            end
        end)
    end,
    Def.ActorFrame{
        Name="Group",
        ShowMessageCommand=function(self) self:queuecommand("Update") end,
    Def.Quad{
        Name="Controller",
        UpdateCommand=function(self)
            n = n + 1
            target:y(200 * math.cos(n * 0.02)):z(-100 - 100 * math.sin(n * 0.02))
            shadow:x(target:GetX()):y(target:GetY()):z(target:GetZ())
            shadow:diffusealpha((100 - 0.2 * (480 - target:GetY())) * 0.01)
            self:sleep(1/50):queuecommand("Update")
        end,
    },
    Def.Quad{
        Name="Shadow",
        InitCommand=function(self) shadow=self end,
        OnCommand=function(self) self:zoomto(64,64) end,
    },
    Def.ActorFrame{
        Name="Target",
        InitCommand=function(self) target=self; self:x(160):y(160) end,
        Def.Quad{OnCommand=function(self) self:zoomto(64,64) end},
    },
    },
}

local player, target
local fired = false
return Def.ActorFrame{
    Name="Root",
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            if beat >= .5 then player = SCREENMAN:GetTopScreen():GetChild("PlayerP1") end
            if beat >= 1 and not fired then MESSAGEMAN:Broadcast("Seen"); fired = true end
        end)
    end,
    Def.Quad{
        Name="Oscillator", InitCommand=cmd(xy,100,100;setsize,32,32),
        OnCommand=cmd(rotationz,4;sleep,.3;queuecommand,"Rotate"),
        RotateCommand=cmd(rotationz,-4;sleep,.3;queuecommand,"On"),
    },
    Def.Quad{
        Name="Target", InitCommand=function(self) target = self; self:xy(300,200):setsize(32,32) end,
        ApplyCommand=cmd(rotationz,30),
    },
    Def.Quad{
        Name="Receiver", InitCommand=cmd(xy,400,200;setsize,32,32),
        UnsentMessageCommand=function(self)
            player:GetName()
            self:zoomx(.6):queuecommand("Apply")
            target:queuecommand("Apply")
        end,
        ApplyCommand=cmd(zoomy,.7),
        SeenMessageCommand=function(self)
            player:GetName()
            self:diffusealpha(.4)
            SOUND:PlayOnce("deferred-hit.wav")
        end,
        UnsentSoundMessageCommand=function(self)
            player:GetName()
            self:diffusealpha(.8):sleep(.25):diffusealpha(0)
            SOUND:PlayOnce("deferred-hit.wav")
            SOUND:PlayOnce("deferred-hit.wav")
        end,
    },
}

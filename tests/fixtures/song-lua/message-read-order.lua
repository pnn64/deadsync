local before, after
local fired = false
local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
return Def.ActorFrame{
    Def.Actor{
        Name='Before', InitCommand=function(self) before=self end,
        KickMessageCommand=function(self) self:sleep(0):x(1):linear(0.2):x(0) end,
    },
    Def.Actor{
        Name='Reader', OnCommand=function(self) self:queuecommand('Update') end,
        UpdateCommand=function(self)
            local beat=GAMESTATE:GetSongBeat()
            options:Invert(before:GetX(),10000)
            options:Alternate(after:GetX(),10000)
            if beat>=0.32 and not fired then fired=true;MESSAGEMAN:Broadcast('Kick') end
            self:sleep(1/75):queuecommand('Update')
        end,
    },
    Def.Actor{
        Name='After', InitCommand=function(self) after=self end,
        KickMessageCommand=function(self) self:sleep(0):x(1):linear(0.2):x(0) end,
    },
}

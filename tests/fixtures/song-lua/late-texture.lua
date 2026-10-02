local capture, fired, flashed = nil, false, false
return Def.ActorFrame{
    Def.Sprite{
        Name="ScreenCopy",
        BindMessageCommand=function(self)
            self:SetTexture(capture:GetTexture()):xy(100, 100)
        end,
    },
    Def.Sprite{
        Name="GlowCopy",
        BindMessageCommand=function(self)
            self:SetTexture(capture:GetTexture()):xy(100, 200):basezoomx(1.03):basezoomy(1.03):diffusealpha(0)
        end,
        FlashMessageCommand=cmd(stoptweening;linear,0.4;diffusealpha,0.9;linear,0.2;diffusealpha,0),
    },
    Def.ActorFrameTexture{
        Name="Capture",
        InitCommand=function(self)
            capture=self
            self:SetWidth(96):SetHeight(48):EnableAlphaBuffer(true):Create()
        end,
        Def.ActorProxy{
            Name="PlayerCopy",
            BindMessageCommand=function(self)
                local player=SCREENMAN:GetTopScreen():GetChild("PlayerP1")
                if player and player:GetVisible() and player:GetDiffuseAlpha()~=0 then
                    self:SetTarget(player)
                    player:visible(false)
                end
            end,
        },
    },
    Def.Actor{
        OnCommand=cmd(queuecommand,"Update"),
        UpdateCommand=function(self)
            local beat=GAMESTATE:GetSongBeat()
            if not fired and beat>=0.2 then fired=true;MESSAGEMAN:Broadcast("Bind") end
            if not flashed and beat>=0.4 then flashed=true;MESSAGEMAN:Broadcast("Flash") end
            self:sleep(1/60):queuecommand("Update")
        end,
    },
}

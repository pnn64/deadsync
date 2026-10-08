local sent = 0
return Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            local seconds = GAMESTATE:GetSongPosition():GetMusicSeconds()
            if sent == 0 and seconds >= 1 then
                sent = 1; MESSAGEMAN:Broadcast('Begin')
            elseif sent == 1 and seconds >= 2 then
                sent = 2; MESSAGEMAN:Broadcast('Fade')
            end
        end)
    end,
    Def.Quad {
        Name = 'LateWrapper',
        InitCommand = function(self) self:setsize(40,20):xy(400,240) end,
        BeginMessageCommand = function(self)
            self:AddWrapperState():vibrate():effectmagnitude(10,10,10)
            self:linear(3):y(240)
        end,
        FadeMessageCommand = function(self)
            self:accelerate(0.3):zoom(3)
            self:GetWrapperState(1):linear(0.5):diffusealpha(0)
        end,
    },
}

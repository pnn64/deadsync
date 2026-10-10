local sent = 0
return Def.ActorFrame {
    Name = 'Root',
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            local seconds = GAMESTATE:GetSongPosition():GetMusicSeconds()
            if (sent == 0 and seconds >= 1) or (sent == 1 and seconds >= 3.75) then
                sent = sent + 1
                MESSAGEMAN:Broadcast('Trigger')
            end
        end)
    end,
    Def.Quad {
        Name = 'MessageWag',
        OnCommand = function(self) self:setsize(40,20):xy(400,240) end,
        TriggerMessageCommand = function(self)
            self:stopeffect():visible(true):accelerate(0.25):addy(350)
                :queuecommand('Waggy'):linear(2.5):addy(-500):linear(0.01):y(-200)
        end,
        WaggyCommand = function(self)
            self:wag():effectmagnitude(30,0,15):effectperiod(0.5)
        end,
    },
}

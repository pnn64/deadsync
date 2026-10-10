local sent = 0
return Def.ActorFrame {
    Name = 'Root',
    OnCommand = function(self) self:sleep(1):queuecommand('Send') end,
    SendCommand = function(self)
        sent = sent + 1
        MESSAGEMAN:Broadcast(sent == 1 and 'Start' or 'Stop')
        if sent == 1 then self:sleep(2):queuecommand('Send') end
    end,
    Def.Quad {
        Name = 'MessageWag',
        OnCommand = function(self) self:setsize(40,20):xy(400,240) end,
        StartMessageCommand = function(self)
            self:stopeffect():visible(true):accelerate(0.25):addy(350)
                :queuecommand('Waggy'):linear(2.5):addy(-500):linear(0.01):y(-200)
        end,
        StopMessageCommand = function(self)
            self:stopeffect():visible(true):accelerate(0.25):addy(350)
            self:decelerate(1.5):addy(-500):linear(0.01):y(-150)
        end,
        WaggyCommand = function(self)
            self:wag():effectmagnitude(30,0,15):effectperiod(0.5)
        end,
    },
}

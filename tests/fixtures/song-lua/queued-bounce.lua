local started = false
local nextBeat = 5
local function bounce(self)
    local beat = GAMESTATE:GetSongBeat()
    local span = math.max(nextBeat - beat, 0.1) * 60/210
    self:decelerate(span/2):addy(-90):accelerate(span/2):y(240)
    if beat < 8 then self:queuecommand("Bounce") end
end
return Def.ActorFrame{
    Name="Root",
    Def.Actor{
        Name="Driver",
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                local beat = GAMESTATE:GetSongBeat()
                if beat > nextBeat - .5 then nextBeat = nextBeat + 1 end
                if not started and beat >= 1 then
                    started = true
                    MESSAGEMAN:Broadcast("StartBounce")
                end
            end)
        end,
    },
    Def.Quad{
        Name="First",
        OnCommand=function(self) self:setsize(16,16):xy(100,-40) end,
        StartBounceMessageCommand=function(self)
            self:sleep(.6):linear(60/210):y(240):queuecommand("Bounce")
        end,
        BounceCommand=bounce,
    },
    Def.Quad{
        Name="Second",
        OnCommand=function(self) self:setsize(16,16):xy(300,-40) end,
        StartBounceMessageCommand=function(self)
            self:sleep(.6):linear(60/210):y(-40):queuecommand("Bounce")
        end,
        BounceCommand=bounce,
    },
}

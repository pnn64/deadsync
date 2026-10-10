local cursor, sent = 1, 0
local root = Def.ActorFrame {
    Name = 'Root',
    OnCommand = function(self) self:sleep(0.5):queuecommand('Send') end,
    SendCommand = function(self)
        sent = sent + 1
        MESSAGEMAN:Broadcast('Next')
        if sent < 8 then self:sleep(0.5):queuecommand('Send') end
    end,
}
for i = 1, 8 do
    for side = 1, 2 do
        root[#root + 1] = Def.Quad {
            Name = 'Pair' .. i .. 'Side' .. side,
            OnCommand = function(self) self:setsize(10,10):xy(i*40,side*60):zoom(0) end,
            NextMessageCommand = function(self)
                if cursor == i then
                    self:linear(0.05):zoom(math.random()+cursor/8)
                    self:AddWrapperState():vibrate():effectmagnitude(5,7,0)
                    cursor = cursor + 1
                end
            end,
        }
    end
end
return root

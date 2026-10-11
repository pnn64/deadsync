local players = {}
local root = Def.ActorFrame{
    BeginCommand = function(self)
        self:SetDrawByZPosition(true):SetFOV(45)
        for pn = 1, 2 do
            players[pn] = SCREENMAN:GetTopScreen():GetChild('PlayerP' .. pn)
            players[pn]:visible(false)
            players[pn]:GetChild('Judgment'):visible(false)
        end
    end,
}
for pn = 1, 2 do
    local player = pn
    root[#root + 1] = Def.ActorProxy{
        Name = 'ProxyP' .. player,
        InitCommand = function(self)
            self:AddWrapperState()
            self:AddWrapperState()
        end,
        BeginCommand = function(self)
            self:SetTarget(players[player])
            assert(self:GetTarget() == players[player])
        end,
        OnCommand = function(self)
            local target = self:GetTarget()
            self:GetWrapperState(1):xy(-target:GetX(), -target:GetY())
            self:GetWrapperState(2):xy(SCREEN_WIDTH * (0.5 * player - 0.25), SCREEN_HEIGHT * 0.5)
        end,
    }
end
return root

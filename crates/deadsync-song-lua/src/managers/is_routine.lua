return function()
    local style = GAMESTATE:GetCurrentStyle()
    if style and style:GetStyleType() == "StyleType_TwoPlayersSharedSides" then
        return true
    end
    return false
end
return Def.ActorFrame{
    OnCommand=function(self)
        local done = false
        self:SetUpdateFunction(function()
            if done or GAMESTATE:GetCurMusicSeconds() < 0.5 then return end
            for index, name in ipairs({"PlayerP1", "PlayerP2"}) do
                local player = SCREENMAN:GetTopScreen():GetChild(name)
                local combo = player:GetChild("Combo")
                local number = combo:GetChild("Number")
                assert(number == combo:GetChildren().Number)
                assert(number:GetParent() == combo and number:GetName() == "Number")
                assert(type(number.GetText) == "function" and number.GetChild == nil)
                assert(combo:GetChild("missing") == nil and combo:GetChild("") == nil)
                number:visible(false)
                player:x(200 + index * 100)
            end
            done = true
        end)
    end,
}

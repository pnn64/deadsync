local sprites = {}
return Def.ActorFrame{
    OnCommand=function(self)
        for i, name in ipairs({"PlayerP1", "PlayerP2"}) do
            local judgment = SCREENMAN:GetTopScreen():GetChild(name):GetChild("Judgment")
            assert(judgment:GetChild("Judgment") == nil)
            assert(judgment:GetChild("") == nil)
            rec_print_children(judgment)
            sprites[i] = judgment:GetChild("JudgmentWithOffsets")
            assert(sprites[i]:GetParent() == judgment)
            sprites[i]:Load("Normal 2x6.png")
            assert(sprites[i]:GetWidth() == 64 and sprites[i]:GetHeight() == 64)
        end
        local phase = 0
        self:SetUpdateFunction(function()
            local seconds = GAMESTATE:GetCurMusicSeconds()
            if phase == 0 and seconds >= 0.05 then
                phase = 1
                for _, sprite in ipairs(sprites) do sprite:Load("Fake 2x6.png") end
            elseif phase == 1 and seconds >= 0.1 then
                phase = 2
                for _, sprite in ipairs(sprites) do sprite:Load("Normal 2x6.png") end
            end
        end)
    end,
}

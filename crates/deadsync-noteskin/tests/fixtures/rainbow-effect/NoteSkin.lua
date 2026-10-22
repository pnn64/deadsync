return { Load = function()
    if Var "Element" == "Receptor" then
        return LoadActor(NOTESKIN:GetPath("Down", "Receptor"))
    end
    return Def.Sprite { Texture=NOTESKIN:GetPath("", "White") }
end }

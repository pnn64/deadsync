return { Load = function()
    if Var "Element" == "Explosion" then
        return LoadActor(NOTESKIN:GetPath("", "Fallback Explosion"))
    end
    return LoadActor(NOTESKIN:GetPath("Down", "Tap Explosion"))
end }

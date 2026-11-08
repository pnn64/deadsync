local skin = {}
function skin.Load()
    local button = Var "Button"
    local element = Var "Element"
    if element == "Tap Mine" then
        local actor = loadfile(NOTESKIN:GetPath("Down", "Tap Mine"))()
        actor.BaseRotationZ = ({Left=90, Down=0, Up=180, Right=-90})[button]
        return actor
    elseif element == "Explosion" then
        return LoadActor(NOTESKIN:GetPath("", "Fallback Explosion"))
    elseif element == "HitMine Explosion" then
        return Def.Sprite { Texture=NOTESKIN:GetPath("Down", "HitMine Explosion") }
    elseif element == "Tap Note" or element == "Receptor" then
        return Def.Sprite { Texture=NOTESKIN:GetPath("_Down", "Core") }
    end
    return Def.Actor {}
end
return skin

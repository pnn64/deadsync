local skin = {}
function skin.Load()
    local button = Var "Button"
    local element = Var "Element"
    local tap = element == "Tap Note" or element == "Tap Fake"
        or element == "Hold Head Active" or element == "Hold Head Inactive"
        or element == "Roll Head Active" or element == "Roll Head Inactive"
    if tap then
        local actor
        if element == "Tap Fake" then
            actor = LoadActor(NOTESKIN:GetPath("_Down", "Tap Note"))
        else
            actor = LoadActor(NOTESKIN:GetPath("Down", "Tap Note"))
        end
        actor.BaseRotationZ = ({Left=90, Down=0, Up=180, Right=-90})[button]
        if button == "Up" then actor.BaseRotationY = 180 end
        if button == "Right" then actor.BaseRotationX = 180 end
        return actor
    end
    if string.find(element, " Body ") or string.find(element, " Bottomcap ") then
        return LoadActor(NOTESKIN:GetPath(button, element))
    end
    if element == "Receptor" then
        return Def.Sprite { Texture=NOTESKIN:GetPath("_Down", "Tap Note") }
    end
    return Def.Actor {}
end
return skin

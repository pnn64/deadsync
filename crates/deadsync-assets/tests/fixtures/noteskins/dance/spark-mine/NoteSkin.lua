local skin = {}
skin.ButtonRedir = { Left="Down", Down="Down", Up="Down", Right="Down" }
skin.PartsToRotate = { ["Tap Mine"] = true }
skin.Rotate = { Left=90, Down=0, Up=180, Right=-90 }

function skin.Load()
    local button = Var "Button"
    local element = Var "Element"
    local load_button = skin.ButtonRedir[button] or button
    local actor = loadfile(NOTESKIN:GetPath(load_button, element))
    if type(actor) == "function" then
        actor = actor(nil)
    else
        actor = Def.Sprite { Texture=NOTESKIN:GetPath(load_button, element) }
    end
    if skin.PartsToRotate[element] then
        actor.BaseRotationZ = skin.Rotate[button]
    end
    return actor
end

return skin

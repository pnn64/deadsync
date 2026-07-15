local skin = {}
skin.ButtonRedir = { Left="Down", Down="Down", Up="Down", Right="Down" }
skin.PartsToRotate = { Flash=true, Glow=true }
skin.Rotate = { Left=90, Down=0, Up=180, Right=-90 }

function skin.Load()
    local button = Var "Button"
    local element = Var "Element"
    local load_button = skin.ButtonRedir[button] or button
    local actor_file = loadfile(NOTESKIN:GetPath(load_button, element))
    local actor
    if type(actor_file) == "function" then
        actor = actor_file(nil)
    else
        actor = Def.Sprite { Texture=NOTESKIN:GetPath(load_button, element) }
    end
    if skin.PartsToRotate[element] then
        actor.BaseRotationZ = skin.Rotate[button]
    end
    return actor
end

return skin

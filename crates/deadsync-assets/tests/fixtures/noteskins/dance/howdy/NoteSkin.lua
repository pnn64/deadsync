local skin = {}
skin.ButtonRedir = { Left="Down", Down="Down", Up="Down", Right="Down" }

function skin.Load()
    local button = Var "Button"
    local element = Var "Element"
    local load_button = skin.ButtonRedir[button] or button
    local actor_file = loadfile(NOTESKIN:GetPath(load_button, element))
    if type(actor_file) == "function" then
        return actor_file(nil)
    end
    return Def.Sprite { Texture=NOTESKIN:GetPath(load_button, element) }
end

return skin

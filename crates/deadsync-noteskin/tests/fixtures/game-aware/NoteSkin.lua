-- Synthetic loader: startup reads have no actor request context in ITGmania.
local Noteskin = ... or {}
local startup_button = Var "Button"
local startup_element = Var "Element"
local game = GAMESTATE:GetCurrentGame():GetName()

local redirects = { Left = "Outer", Down = "Inner", Up = "Outer", Right = "Inner" }
if game == "pump" then
    redirects = { DownLeft = "Outer", UpLeft = "Inner", Center = "Outer", UpRight = "Inner", DownRight = "Outer" }
end

Noteskin.Load = function()
    assert(startup_button == nil and startup_element == nil)
    assert(Var "MissingVariable" == nil)
    local button = Var "Button"
    local element = Var "Element"
    assert(type(button) == "string" and type(element) == "string")
    assert(GAMESTATE:GetCurrentGame():GetName() == game)

    if string.find(element, "Tail") then
        if Var "SpriteOnly" then return LoadActor("_blank") end
        return Def.Actor {}
    end
    if string.find(element, "Head") then element = "Tap Note" end
    local actor = LoadActor(NOTESKIN:GetPath(redirects[button] or button, element))
    actor.BaseRotationZ = game == "pump" and 90 or 0
    return actor
end
return Noteskin

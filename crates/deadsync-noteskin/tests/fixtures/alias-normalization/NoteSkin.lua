local skin = ... or {}
skin.Rotate = {left=90, down=0, up=180, right=270}
skin.Blank = {['tap note']=false, ['hold head active']=true}
for i=1,64 do skin.Rotate['extra'..i]=i end
skin.Load = function()
    local button, element = Var('Button'), Var('Element')
    assert(skin.Rotate[button] ~= nil)
    if element == 'Tap Note' then assert(skin.Blank[element] == false) end
    if element == 'Hold Head Active' then assert(skin.Blank[element] == true) end
    return Def.Sprite {
        Texture = '_blank',
        BaseRotationZ = skin.Rotate[button],
        InitCommand = function(self) self:x(skin.Rotate[button]):y(2) end,
    }
end
return skin

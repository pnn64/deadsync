
return Def.Actor { OnCommand=function()
    local screen = SCREENMAN:GetTopScreen()
    local header, footer = screen:GetChild('Header'), screen:GetChild('Footer')
    assert(header:GetParent() == screen and footer:GetParent() == screen)
    assert(header:GetNumChildren() == 2)
    local children = header:GetChildren()
    assert(children[''] and children.HeaderText)
    assert(children.HeaderText:GetX() == 10 and children.HeaderText:GetY() == 15)
    assert(children.HeaderText:GetText() == '', 'missing native HeaderText remains empty')
    assert(math.abs(children.HeaderText:GetTweenTimeLeft() - 0.43) < 0.00001)
    assert(screen:GetChild('PlayerOptionsP1'):GetText():find('Overhead', 1, true),
        'hidden engine options text is initialized before authored actors')
    assert(header:GetTweenTimeLeft() == math.huge)
    -- Quad inherits Sprite's Lua binding in ITGmania.
    assert(tostring(footer):match('Sprite') and footer:GetY() == 32,
        tostring(footer)..' y='..tostring(footer:GetY()))
    assert(footer:GetTweenTimeLeft() == math.huge)
    MESSAGEMAN:Broadcast('NativeDecorations')
end }

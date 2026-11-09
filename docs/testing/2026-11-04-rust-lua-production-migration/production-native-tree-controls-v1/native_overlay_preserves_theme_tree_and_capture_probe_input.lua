
return Def.Actor { OnCommand=function(self)
    local overlay = SCREENMAN:GetTopScreen():GetChild('Overlay')
    local children = overlay:GetChildren()
    local probe = children.ITGmaniaHarnessTraceProbe
    local theme = probe and children[''] or overlay
    assert(theme:GetNumChildren() == 6, 'base single-player Simply Love overlay retains its logic actors')
    local logic = theme:GetChildren()['']
    assert(#logic == 6 and tostring(logic[1]):match('ActorFrame'))
    assert(tostring(logic[2]):match('Actor %('))
    assert(overlay:GetNumChildren() == (probe and 2 or 6))
    if probe then assert(probe:GetNumChildren() == 0 and probe:GetParent() == overlay) end
    MESSAGEMAN:Broadcast(probe and 'CapturedOverlay' or 'NormalOverlay')
end }

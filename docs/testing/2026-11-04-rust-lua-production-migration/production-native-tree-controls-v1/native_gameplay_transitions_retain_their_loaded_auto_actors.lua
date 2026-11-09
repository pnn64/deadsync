
return Def.Actor { OnCommand=function()
    local screen = SCREENMAN:GetTopScreen()
    local transitions = screen:GetChildren()['']
    assert(#transitions == 5)
    local course, blank, hidden = 0, 0, 0
    for _,wrapper in ipairs(transitions) do
        assert(wrapper:GetVisible() and wrapper:GetParent() == screen)
        assert(wrapper:GetNumChildren() == 1 and wrapper:GetTweenTimeLeft() == 0)
        local child = wrapper:GetChildren()['']
        assert(child:GetParent() == wrapper)
        if tostring(child):match('ActorFrame') then
            course = course + 1
            assert(child:GetNumChildren() == 2)
            local visuals = child:GetChildren()['']
            assert(#visuals == 2 and visuals[1]:GetParent() == child)
            assert(visuals[1]:GetZoomedWidth() == _screen.w)
            assert(visuals[1]:GetZoomedHeight() == _screen.h)
        else
            assert(tostring(child):match('Actor %('))
            blank = blank + 1
            if not child:GetVisible() then hidden = hidden + 1 end
        end
    end
    assert(course == 1 and blank == 4 and hidden == 1)
    MESSAGEMAN:Broadcast('NativeTransitionChildren')
end }

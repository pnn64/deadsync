return Def.ActorFrame {
    OnCommand = function(self)
        for _, player in ipairs(GAMESTATE:GetHumanPlayers()) do
            local field = SCREENMAN:GetTopScreen():GetChild("Player" .. ToEnumShortString(player)):GetChild("NoteField")
            for _, column in ipairs(field:get_column_actors()) do
                local handler = column:get_rot_handler()
                handler:set_spline_mode("NoteColumnSplineMode_Position")
                handler:get_spline():set_loop(true):set_size(1):set_point(1, {0, 0, math.pi})
            end
        end
    end,
}

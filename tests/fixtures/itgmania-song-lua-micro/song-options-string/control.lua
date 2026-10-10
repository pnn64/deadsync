return Def.ActorFrame{
    InitCommand=function(self)
        for _, rate in ipairs({1, 1.25, 1.2, 2, 1.234, 1.005}) do
            -- Stage modifiers synchronously update Song and Current options.
            GAMESTATE:ApplyStageModifiers(PLAYER_1, tostring(rate) .. 'xMusic')
            MESSAGEMAN:Broadcast('SongOptionsString:' .. GAMESTATE:GetSongOptionsString(), {rate=rate})
        end
    end
}

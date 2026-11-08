-- Repeated sound-manager calls exercise path capture and deduplication.
return Def.ActorFrame{
    Name='SoundPaths',
    OnCommand=function(self)
        for i = 1, 64 do
            SOUND:PlayOnce('asset-string.wav')
            SOUND:PlayMusicPart('asset-string.wav', 0, 0)
        end
    end,
}

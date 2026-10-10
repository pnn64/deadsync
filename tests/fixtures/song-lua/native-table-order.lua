local records = {
    {TimeSpacing=0, MaxScrollBPM=0, ScrollSpeed=1, ScrollBPM=200, XMod=1, CMod=200, MMod=200},
    {MMod=200, CMod=200, XMod=1, ScrollBPM=200, ScrollSpeed=1, MaxScrollBPM=0, TimeSpacing=0},
    {MMod=200, MaxScrollBPM=0, TimeSpacing=0, ScrollSpeed=4},
    {ScrollSpeed=4, TimeSpacing=0, MaxScrollBPM=0, MMod=200},
}
return Def.ActorFrame{
    OnCommand = function(self)
        for _, player in ipairs({PLAYER_1, PLAYER_2}) do
            local options = GAMESTATE:GetPlayerState(player):GetPlayerOptions("ModsLevel_Song")
            for _, record in ipairs(records) do
                options:XMod(1)
                for name, amount in pairs(record) do options[name](options, amount) end
            end
            options:XMod(4)
        end
    end,
}

mod_time = {
    {0, 1, "*100 no dark", "len", 1},
}

mods_ease = {
    {1, 1, 0, 100, "tiny", "len", ease.outCirc, 1},
    {2, 1, 100, 0, "tiny", "len", ease.inCirc, 1},
    {3, 1, 0, 100, "Bumpy1", "len", ease.outElastic, 1},
    {4, 1, 0, 100, "Bumpy4", "len", ease.outQuad, 1},
    {5, 1, 0, 100, "drunk", "len", ease.outQuad, 1},
    {6, 1, 0, 100, "tipsy", "len", ease.outQuad, 1},
    {7, 1, 0, 100, "brake", "len", ease.outQuad, 1},
    {8, 1, 0, 100, "beat", "len", ease.outQuad, 1},
    {9, 1, 0, 100, "stealth", "len", ease.outQuad, 1},
    {10, 1, 0, 100, "movey1", "len", ease.outQuad, 1},
    {11, 1, 0, 100, "confusionoffset1", "len", ease.outQuad, 1},
}

return Def.ActorFrame{
    Def.Quad{Name="ModFixtureOverlay"},
}

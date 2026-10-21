-- A one-pixel backdrop scaled to the logical screen height.
return Def.ActorFrame {
    Def.Sprite {
        Texture="_backdrop";
        InitCommand=function(self)
            self:valign(1):zoomy(SCREEN_HEIGHT):addrotationz(180)
        end;
    };
    Def.Sprite {
        Texture="_arrow";
    };
}

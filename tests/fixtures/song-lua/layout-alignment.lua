return Def.ActorFrame{
    Def.Quad{Name="TopLeft", OnCommand=cmd(xy,320,240;zoomto,80,60;horizalign,left;vertalign,top)},
    Def.Quad{Name="TopRight", OnCommand=function(self) self:xy(320,240):zoomto(80,60):horizalign("HorizAlign_Right"):vertalign("VertAlign_Top") end},
    Def.Quad{Name="BottomLeft", OnCommand=function(self) self:xy(320,240):zoomto(80,60):horizalign("left"):vertalign("bottom") end},
    Def.Quad{Name="BottomRight", OnCommand=cmd(xy,320,240;zoomto,80,60;horizalign,right;vertalign,bottom)},
    Def.Quad{Name="Center", OnCommand=function(self) self:xy(320,240):zoomto(80,60):horizalign("center"):vertalign("middle") end},
    Def.Quad{Name="Numeric", OnCommand=function(self) self:xy(320,240):zoomto(80,60):halign(0.25):valign(0.75) end},
}

return Def.ActorFrame{
Def.Sprite{Texture="tiny.png"},
Def.Sprite{Name="getter-1", Texture="tiny.png",
        OnCommand=function(self)
            local texture = self:GetTexture()
            local sizes = string.format("%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
                texture:GetSourceWidth(), texture:GetSourceHeight(),
                texture:GetImageWidth(), texture:GetImageHeight(),
                texture:GetTextureWidth(), texture:GetTextureHeight())
            self:Load("sheet 2x1.png")
            sizes = sizes .. string.format(":%.0f:%.0f", texture:GetImageWidth(), texture:GetImageHeight())
            assert(sizes == "3:2:8:8:8:8:8:8", "native dimensions tiny: " .. sizes)
            self:x(101)
        end,
    },
Def.Sprite{Texture="tiny-wide.png"},
Def.Sprite{Name="getter-2", Texture="tiny-wide.png",
        OnCommand=function(self)
            local texture = self:GetTexture()
            local sizes = string.format("%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
                texture:GetSourceWidth(), texture:GetSourceHeight(),
                texture:GetImageWidth(), texture:GetImageHeight(),
                texture:GetTextureWidth(), texture:GetTextureHeight())
            self:Load("sheet 2x1.png")
            sizes = sizes .. string.format(":%.0f:%.0f", texture:GetImageWidth(), texture:GetImageHeight())
            assert(sizes == "9:3:16:8:16:8:16:8", "native dimensions tiny-wide: " .. sizes)
            self:x(102)
        end,
    },
Def.Sprite{Texture="tiny-tall.png"},
Def.Sprite{Name="getter-3", Texture="tiny-tall.png",
        OnCommand=function(self)
            local texture = self:GetTexture()
            local sizes = string.format("%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
                texture:GetSourceWidth(), texture:GetSourceHeight(),
                texture:GetImageWidth(), texture:GetImageHeight(),
                texture:GetTextureWidth(), texture:GetTextureHeight())
            self:Load("sheet 2x1.png")
            sizes = sizes .. string.format(":%.0f:%.0f", texture:GetImageWidth(), texture:GetImageHeight())
            assert(sizes == "3:9:8:16:8:16:8:16", "native dimensions tiny-tall: " .. sizes)
            self:x(103)
        end,
    },
Def.Sprite{Texture="padded.png"},
Def.Sprite{Name="getter-4", Texture="padded.png",
        OnCommand=function(self)
            local texture = self:GetTexture()
            local sizes = string.format("%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
                texture:GetSourceWidth(), texture:GetSourceHeight(),
                texture:GetImageWidth(), texture:GetImageHeight(),
                texture:GetTextureWidth(), texture:GetTextureHeight())
            self:Load("sheet 2x1.png")
            sizes = sizes .. string.format(":%.0f:%.0f", texture:GetImageWidth(), texture:GetImageHeight())
            assert(sizes == "7:9:7:9:8:16:7:9", "native dimensions padded: " .. sizes)
            self:x(104)
        end,
    },
Def.Sprite{Texture="panel (stretch).png"},
Def.Sprite{Name="getter-5", Texture="panel (stretch).png",
        OnCommand=function(self)
            local texture = self:GetTexture()
            local sizes = string.format("%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
                texture:GetSourceWidth(), texture:GetSourceHeight(),
                texture:GetImageWidth(), texture:GetImageHeight(),
                texture:GetTextureWidth(), texture:GetTextureHeight())
            self:Load("sheet 2x1.png")
            sizes = sizes .. string.format(":%.0f:%.0f", texture:GetImageWidth(), texture:GetImageHeight())
            assert(sizes == "5:9:8:16:8:16:8:16", "native dimensions stretch: " .. sizes)
            self:x(105)
        end,
    },
Def.Sprite{Texture="hires (doubleres).png"},
Def.Sprite{Name="getter-6", Texture="hires (doubleres).png",
        OnCommand=function(self)
            local texture = self:GetTexture()
            local sizes = string.format("%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
                texture:GetSourceWidth(), texture:GetSourceHeight(),
                texture:GetImageWidth(), texture:GetImageHeight(),
                texture:GetTextureWidth(), texture:GetTextureHeight())
            self:Load("sheet 2x1.png")
            sizes = sizes .. string.format(":%.0f:%.0f", texture:GetImageWidth(), texture:GetImageHeight())
            assert(sizes == "5:9:10:18:16:32:10:18", "native dimensions doubleres: " .. sizes)
            self:x(106)
        end,
    },
Def.Sprite{Texture="source (res 53x12).png"},
Def.Sprite{Name="getter-7", Texture="source (res 53x12).png",
        OnCommand=function(self)
            local texture = self:GetTexture()
            local sizes = string.format("%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
                texture:GetSourceWidth(), texture:GetSourceHeight(),
                texture:GetImageWidth(), texture:GetImageHeight(),
                texture:GetTextureWidth(), texture:GetTextureHeight())
            self:Load("sheet 2x1.png")
            sizes = sizes .. string.format(":%.0f:%.0f", texture:GetImageWidth(), texture:GetImageHeight())
            assert(sizes == "53:12:8:8:8:8:8:8", "native dimensions resolution: " .. sizes)
            self:x(107)
        end,
    },
Def.Sprite{Texture="capped.png"},
Def.Sprite{Name="getter-8", Texture="capped.png",
        OnCommand=function(self)
            local texture = self:GetTexture()
            local sizes = string.format("%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
                texture:GetSourceWidth(), texture:GetSourceHeight(),
                texture:GetImageWidth(), texture:GetImageHeight(),
                texture:GetTextureWidth(), texture:GetTextureHeight())
            self:Load("sheet 2x1.png")
            sizes = sizes .. string.format(":%.0f:%.0f", texture:GetImageWidth(), texture:GetImageHeight())
            assert(sizes == "2049:9:2048:9:2048:16:2048:9", "native dimensions capped: " .. sizes)
            self:x(108)
        end,
    },
Def.Sprite{Texture="capped-tiny.png"},
Def.Sprite{Name="getter-9", Texture="capped-tiny.png",
        OnCommand=function(self)
            local texture = self:GetTexture()
            local sizes = string.format("%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
                texture:GetSourceWidth(), texture:GetSourceHeight(),
                texture:GetImageWidth(), texture:GetImageHeight(),
                texture:GetTextureWidth(), texture:GetTextureHeight())
            self:Load("sheet 2x1.png")
            sizes = sizes .. string.format(":%.0f:%.0f", texture:GetImageWidth(), texture:GetImageHeight())
            assert(sizes == "2049:3:2048:8:2048:8:2048:8", "native dimensions capped-tiny: " .. sizes)
            self:x(109)
        end,
    },
Def.Sprite{Texture="sheet 2x1.png"},
Def.Sprite{Name="getter-10", Texture="sheet 2x1.png",
        OnCommand=function(self)
            local texture = self:GetTexture()
            local sizes = string.format("%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
                texture:GetSourceWidth(), texture:GetSourceHeight(),
                texture:GetImageWidth(), texture:GetImageHeight(),
                texture:GetTextureWidth(), texture:GetTextureHeight())
            self:Load("sheet 2x1.png")
            sizes = sizes .. string.format(":%.0f:%.0f", texture:GetImageWidth(), texture:GetImageHeight())
            assert(sizes == "16:8:16:8:16:8:16:8", "native dimensions sheet: " .. sizes)
            self:x(110)
        end,
    }
}

-- Compile fixture with wrappers created after their owned actors.
local root = Def.ActorFrame { Name = 'FinalizeRoot' }
for i = 1, 128 do
    root[#root + 1] = Def.Quad {
        Name = 'Finalize' .. i,
        OnCommand = function(self)
            self:xy(i * 2, i % 16):setsize(8, 12)
            self:AddWrapperState():x(i):zoom(.75)
            self:AddWrapperState():y(i % 8):rotationz(i % 12)
        end,
    }
end
return root

load_order = 'script1'
return Def.ActorFrame{
    Name='First',
    InitCommand=function(self)
        assert(load_order == 'script1 child', load_order)
        assert(self:GetParent():GetName() == 'SongForeground', 'root parent')
        assert(next(self:GetParent():GetChildren()) == nil, 'root attached before Init')
        load_order = load_order .. ' root'
    end,
    Def.Quad{
        Name='Child',
        InitCommand=function(self)
            load_order = load_order .. ' child'
            self:xy(math.random(1,1000),20):setsize(32,32)
        end
    }
}

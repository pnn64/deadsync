assert(load_order == 'script1 child root', load_order)
load_order = load_order .. ' script2'
local random_x = math.random(1,1000)
return Def.Quad{
    Name='Second',
    InitCommand=function(self)
        assert(load_order == 'script1 child root script2', load_order)
        local count = 0
        for _ in pairs(self:GetParent():GetChildren()) do count = count + 1 end
        assert(count == 1, 'prior root missing')
        self:xy(random_x,60):setsize(32,32)
    end
}

local function bounds()
    assert(not pcall(math.random, 0))
    assert(not pcall(math.random, 4, 4))
    assert(not pcall(math.random, 5, 4))
    assert(not pcall(math.random, 1, 2, 3))
    assert(select("#", math.randomseed(1)) == 0)
end
return Def.ActorFrame{
    Def.Quad{
        Name="First",
        InitCommand=function(self)
            bounds()
            self:x(math.random(-10, 10)):y(math.random())
                :z(math.random(1)):zoom(math.random())
        end,
    },
    Def.Quad{
        Name="Twist",
        InitCommand=function(self)
            MersenneTwister.Seed(12345)
            local value
            for i=1,1000 do value=MersenneTwister.Random() end
            self:x(value):y(math.random()):z(math.random(-1000000000, 1000000000))
        end,
    },
    Def.Quad{
        Name="Reseed",
        InitCommand=function(self)
            math.randomseed(1)
            self:x(math.random("-10.9", "10.9")):y(math.random())
                :z(math.random(1)):zoom(math.random())
        end,
    },
}

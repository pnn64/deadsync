local marvelouscolour = color("#ffffff")
local perfectcolour = color("#fff064")
local greatcolour = color("#3fff3f")
local goodcolour = color("#3effbf")

local inneralpha = 0.0


-- Marvelous Flash COMMAND
local function brightflash(thecolour)
	return function(self) self:finishtweening()
		self:diffuse(thecolour):diffusealpha(0.9):zoom(0.7)
		:linear(6/60):zoom(0.95)
		:sleep(0):diffuse(0,0,0,1)
	end
end

-- Regular Flash COMMAND
local function dimflash(thecolour)
	return function(self) self:finishtweening()
		self:diffuse(thecolour):diffusealpha(0.9):zoom(1)
		:linear(9/60):diffuse(0,0,0,1):zoom(1.25)
	end
end

-- Hold Held Emitter COMMAND
local holdflash = NOTESKIN:LoadActor(Var "Button", "Flash Dim")..{ Name="holdflash",
	InitCommand=function(self) self:blend(Blend.Add):diffuse(0,0,0,0) end,
	FlashCommand=dimflash(perfectcolour)
}


------------------------------------------------------

return Def.ActorFrame {
	--Regular Flash
	NOTESKIN:LoadActor(Var "Button", "Flash Dim")..{ 
		JudgmentCommand=function(self) self:finishtweening() end,
		InitCommand=function(self) self:blend(Blend.Add):diffuse(0,0,0,0) end,
		
		W2Command=dimflash(perfectcolour),
		W3Command=dimflash(greatcolour),
		W4Command=dimflash(goodcolour),
	},

	--Marvelous Flash
	NOTESKIN:LoadActor(Var "Button", "Flash Bright")..{
		JudgmentCommand=function(self) self:finishtweening() end,
		InitCommand=function(self) self:blend(Blend.Add):diffuse(0,0,0,0) end,
		
		W1Command=brightflash(marvelouscolour),
		
		--Perfect (Inner Marvelous Flash)
		W2Command=function(self) self:finishtweening()
			self:diffuse(color("#ffffff")):zoom(0.54)
			:linear(3/60):zoom(0.7)
			:sleep(2/60):diffuse(0,0,0,1)
		end,
	},
	
	--Hold Held Emitter
	Def.ActorFrame {
		InitCommand=function(self)
			self.emitting = false
			self.emitnumber = 1
			self.emissions = self:GetChild("holdflash")
		end,
		HoldingOnCommand=function(self) 
			self.emitting = true; 
			self:finishtweening():playcommand("Emit") 
		end,
		HoldingOffCommand=function(self) self.emitting = false end,
		RollOnCommand=function(self) 
			self.emitting = true; 
			self:finishtweening():playcommand("Emit") 
		end,
		RollOffCommand=function(self) self.emitting = false end,
		EmitCommand=function(self)
			self.emissions[self.emitnumber]:finishtweening():playcommand("Flash")
			if self.emitnumber == 3 then self.emitnumber = 1 else self.emitnumber = self.emitnumber+1 end
			if self.emitting then
				self:sleep(4/60):queuecommand("Emit")
			end
		end,
		holdflash, holdflash, holdflash,
	},
	--Hold End Flash
	NOTESKIN:LoadActor(Var "Button", "Flash Bright")..{ 
		JudgmentCommand=function(self) self:finishtweening() end,
		InitCommand=function(self) self:blend(Blend.Add):diffuse(0,0,0,0) end,
		HeldCommand=brightflash(perfectcolour),
	},
	
	--Mine Explosion Commands
	NOTESKIN:LoadActor( Var "Button", "HitMine Explosion" ) .. {
		InitCommand=NOTESKIN:GetMetricA("GhostArrowBright", "MineInitCommand"),
		HitMineCommand=NOTESKIN:GetMetricA("GhostArrowBright", "HitMineCommand"),
	},
}

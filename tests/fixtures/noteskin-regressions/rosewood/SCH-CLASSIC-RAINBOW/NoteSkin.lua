--I am the bone of my noteskin
--Arrows are my body, and explosions are my blood
--I have created over a thousand noteskins
--Unknown to death
--Nor known to life
--Have withstood pain to create many noteskins
--Yet these hands will never hold anything
--So as I pray, Unlimited Stepman Works


local NSKIN = {}

NSKIN.ButtonRedir =
{
	Up = "Down",
	Down = "Down",
	Left = "Down",
	Right = "Down",
	UpLeft = "Down",
	UpRight = "Down"
}

NSKIN.ElementRedir =
{
}

NSKIN.PartsToRotate =
{
	["Receptor"] = true,
	["Flash Bright"] = false,
	["Flash Dim"] = true,
	["Tap Note"] = true,
	["Tap Mine"] = true,
	["Tap Fake"] = true,
	["Tap Lift"] = true,
	["Tap Addition"] = true,
	["Hold Explosion"] = true,
	["Hold Head Active"] = true,
	["Hold Head Inactive"] = true,
	["Roll Explosion"] = true,
	["Roll Head Active"] = true,
	["Roll Head Inactive"] = true
}

NSKIN.Rotate =
{
	Up = 180,
	Down = 0,
	Left = 90,
	Right = -90,
	UpLeft = 135,
	UpRight = 225
}

NSKIN.Blank =
{
	["Hold Topcap Active"] = true,
	["Hold Topcap Inactive"] = true,
	["Roll Topcap Active"] = true,
	["Roll Topcap Inactive"] = true,
	["Hold Tail Active"] = true,
	["Hold Tail Inactive"] = true,
	["Roll Tail Active"] = true,
	["Roll Tail Inactive"] = true
}


function NSKIN.Load()
	local sButton = Var "Button"
	local sElement = Var "Element"
	local sPlayer = Var "Player"

	sElement = string.gsub(sElement, "Simple", "")

	local Reverse = string.find(GAMESTATE:GetPlayerState(sPlayer):GetPlayerOptionsString("ModsLevel_Preferred"):lower(), "reverse")

	local Button = NSKIN.ButtonRedir[sButton] or sButton
	local Element = NSKIN.ElementRedir[sElement] or sElement

	if (string.find(Element, "Active") or
			string.find(Element, "Inactive")) and
			(not string.find(Element, "Head") and
			not string.find(Element, "Roll")) then
		Button = sButton
		if Reverse and sButton == "Up" then Button = "Down" end
		if Reverse and sButton == "Down" then Button = "Up" end
	end

	local Actor = loadfile(NOTESKIN:GetPath(Button, Element))

	if type(Actor) == "function" then
		Actor = Actor(nil)
	else
		Actor = Def.Sprite {
			Texture = NOTESKIN:GetPath(Button, Element),
			HoldUpdateCommand = function(self) self:customtexturerect(0, -(self:GetHoldLength() / self:GetHeight()), 1, 0) end
		}
	end

	if NSKIN.Blank[sElement] then
		Actor = Def.Actor {}
		if Var "SpriteOnly" then
			Actor = Def.Sprite { Texture = NOTESKIN:GetPath("", "_blank") }
		end
	end

	if NSKIN.PartsToRotate[sElement] then
		Actor.BaseRotationZ = NSKIN.Rotate[sButton] or nil
	end

	return Actor
end

return NSKIN

"""Authored API families, not a claim of exhaustive argument or behavior coverage."""

TRANSFORMS = {
    "position": ":x(31):y(-17):z(8):xy(19,23):addx(-4):addy(7):addz(-3)",
    "rotation": ":rotationx(15):rotationy(-25):rotationz(35)",
    "add_rotation": ":addrotationx(12):addrotationy(-9):addrotationz(18)",
    "base_rotation": ":baserotationx(10):baserotationy(-20):baserotationz(30)",
    "euler": ":heading(20):pitch(-10):roll(15)",
    "zoom_axes": ":zoom(0.75):zoomx(-0.5):zoomy(1.25):zoomz(0.5)",
    "base_zoom": ":basezoom(0.8):basezoomx(0.75):basezoomy(1.25):basezoomz(0.5)",
    "size": ":setsize(32,24):SetWidth(40):SetHeight(28)",
    "zoom_to": ":zoomto(40,28)",
    "zoom_width": ":zoomtowidth(48)",
    "zoom_height": ":zoomtoheight(36)",
    "stretch": ":stretchto(-24,-16,32,24)",
    "fit": ":scaletofit(-24,-16,32,24)",
    "cover": ":scaletocover(-24,-16,32,24)",
    "skew": ":skewx(0.2):skewy(-0.15)",
    "alignment": ":halign(0.25):valign(0.75)",
    "named_alignment": ":horizalign('left'):vertalign('bottom')",
    "crop": ":cropleft(0.1):cropright(0.2):croptop(0.15):cropbottom(0.05)",
    "fade": ":fadeleft(0.1):faderight(0.2):fadetop(0.15):fadebottom(0.05)",
}

TWEENS = ("linear", "accelerate", "decelerate", "spring", "bezier1d", "bezier2d",
          "bouncebegin", "bounceend", "smooth", "enum_linear", "enum_accelerate",
          "enum_decelerate", "enum_spring")

def tween_call(kind, duration):
    if kind == "bezier1d":
        return f"tween({duration},'TweenType_Bezier',{{0,0.2,0.8,1}})"
    if kind == "bezier2d":
        return f"tween({duration},'TweenType_Bezier',{{0,0,0.25,0.1,0.75,0.9,1,1}})"
    if kind.startswith("enum_"):
        return f"tween({duration},'TweenType_{kind[5:].capitalize()}')"
    return f"{kind}({duration})"

COLORS = {
    "corners": ":diffuseupperleft(1,0,0,1):diffuseupperright(0,1,0,0.5):diffuselowerleft(0,0,1,0.75):diffuselowerright(1,1,0,1)",
    "horizontal_edges": ":diffuseleftedge(1,0,0,1):diffuserightedge(0,1,1,0.5)",
    "vertical_edges": ":diffusetopedge(1,0,1,1):diffusebottomedge(0,1,0,0.5)",
    "rgb_only": ":diffusealpha(0.5):diffusecolor(0.25,0.75,1,1)",
    "base_alpha": ":basealpha(0.5)",
}

EFFECTS = {
    "none": "", "pulse": ":pulse():effectmagnitude(0.75,1.25,0)",
    "spin": ":spin():effectmagnitude(0,0,30)",
    "wag": ":wag():effectmagnitude(5,10,15)",
    "bounce": ":bounce():effectmagnitude(5,10,0)",
    "bob": ":bob():effectmagnitude(5,10,0)",
    **{mode: f":{mode}():effectcolor1(1,0,0,0.5):effectcolor2(0,1,1,1)"
       for mode in ("diffuseblink", "diffuseshift", "diffuseramp", "glowblink", "glowshift", "glowramp")},
    "rainbow": ":rainbow()", "stopped": ":spin():stopeffect()",
}

# Random vibration remains a named coverage gap: different PRNG algorithms and
# draw order consumption cannot be made identical just by sharing a Lua seed.
EXCLUSIONS = {
    "vibrate": "requires statistical/bounded-motion oracle; excluded from strict equality",
    "interactive Lua": "compiled playback has no runtime VM",
    "ActorProxy/ActorFrameTexture/ActorMultiVertex/Model/BitmapText": "need additional generated assets and comparison support",
    "all PlayerOptions and spline methods": "only selected modifiers, column transforms and position splines generated",
    "all numerical inputs and higher-order combinations": "finite campaigns cannot be exhaustive",
    "mutation and minimization": "not implemented",
}

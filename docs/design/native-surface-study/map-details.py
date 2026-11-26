# SPDX-License-Identifier: MPL-2.0
from draw_review import P, Image, ImageDraw, text, sprite, button
im = Image.new("RGB",(1080,440),"#202223")
draw=ImageDraw.Draw(im)
for col, grid in enumerate((False, True)):
    x=540*col; text(im,(x+18,12),"Terrain" if not grid else "Chunk boundaries",18,bold=True)
    raw=Image.open(P/"alder-reach.png").convert("RGB").crop((224,212,351,254)).resize((508,168),Image.Resampling.NEAREST)
    if grid:
        mask=Image.new("RGBA",raw.size)
        layer=ImageDraw.Draw(mask)
        for axis in range(0,508,64):layer.line((axis,0,axis,168),fill=(230,235,227,68))
        for axis in range((16-212%16)*4,168,64):layer.line((0,axis,508,axis),fill=(230,235,227,68))
        raw=Image.alpha_composite(raw.convert("RGBA"),mask).convert("RGB")
    im.paste(raw,(x+16,45))
text(im,(18,228),"Normal · no observed world",14,"#c5c7c4")
draw.rectangle((18,255,522,418),fill="#2a2c2d")
sprite(im,"gear",40,284,64);text(im,(131,283),"Redstone Workshop",20,bold=True)
text(im,(131,315),"No world entered yet",13,"#bfc1bf");button(im,430,370,"Play")
text(im,(558,228),"Compact · last played",14,"#c5c7c4")
draw.rectangle((558,255,1063,335),fill="#2a2c2d")
sprite(im,"fox",576,278,32);text(im,(622,269),"Timberline Trail",15,bold=True)
text(im,(622,295),"Alder Reach",13,"#c1c3c2")
for j,action in enumerate(("Pin","Map")):
    a=845+49*j;draw.rounded_rectangle((a,278,a+43,308),3,outline="#626664");text(im,(a+8,287),action,12)
button(im,973,278,"Play")
im.save(P/"coverage-and-density.png")

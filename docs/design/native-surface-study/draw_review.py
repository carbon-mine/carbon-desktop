# SPDX-License-Identifier: MPL-2.0
"""Assemble the native captures and draw static, original control alternatives at 96 dpi."""
from pathlib import Path
import os
from PIL import Image, ImageDraw, ImageFont
P = Path(__file__).parent
FONTS = Path(os.getenv("WINDIR", "C:/Windows")) / "Fonts"
def font(n=14,bold=False):
    return ImageFont.truetype(str(FONTS/("seguisb.ttf" if bold else "segoeui.ttf")),n)
def text(im,xy,value,size=14,color="#eeeeeb",bold=False):
    ImageDraw.Draw(im).text(xy,value,font=font(size,bold),fill=color,anchor="lt")
def sprite(im,name,x,y,side):
    icon=Image.open(P/(name+'.png')).convert("RGBA").resize((side,side),Image.Resampling.NEAREST)
    im.paste(icon,(x,y),icon)
def clip(im,box,pos): return im.crop(box),pos

def surface_sheet():
    out=Image.new("RGB",(1080,238),"#191a1b")
    for col,(key,title,notes) in enumerate([("current","Current","Full-height gradient"),("flat","Flat","Single Carbon surface"),("subtle","Subtle","Short fade at the top")]):
        x=col*360
        text(out,(x+12,10),title,19,bold=True);text(out,(x+12,38),notes,13,"#b7b8b8")
        img=Image.open(P/(key+'.png'))
        out.paste(img.crop((200,90,552,250)),(x+4,67))
    out.save(P/"surface-options.png")

def button(im,x,y,label,light=False,style='quiet',pressed=False,focused=False,width=None):
    d=ImageDraw.Draw(im)
    width=width or (76 if label=='Play' else 83)
    if style=='quiet':
        top,bottom,ink=(("#292b2c","#151617","#f7f7f3") if light else ("#e0e1dc","#969995","#222522"))
    elif style=='tonal':
        top,bottom,ink=(("#e8e9e5","#b9bdb7","#262724") if light else ("#373a37","#1d211f","#efefea"))
    else:
        top,bottom,ink=(("#f1f1ef","#b9babc","#222") if light else ("#2c2c2c","#444","#eee"))
    if focused:d.rounded_rectangle((x-3,y-3,x+width+2,y+34),radius=3,outline="#1569ac" if light else "#e7eaf0",width=2)
    step=0 if style=='base' else 3
    d.polygon([(x+2,y),(x+width-2,y),(x+width,y+2),(x+width,y+28),(x+width-2,y+28),(x+width-2,y+31),(x+2,y+31),(x+2,y+29),(x,y+29),(x,y+2)],fill=bottom)
    down=2 if pressed else 0
    d.rectangle((x+1,y+down+1,x+width-1,y+29-step+down),fill=top)
    d.polygon([(x+13,y+9+down),(x+13,y+21+down),(x+22,y+15+down)],fill=ink)
    text(im,(x+30,y+8+down),label,13,ink,True)
    return width

def chooser(im,x,y,light=False):
    c="#4e5050" if not light else "#b4b6b4"; ink="#d2d4d1" if not light else "#333"
    d=ImageDraw.Draw(im); d.rounded_rectangle((x,y,x+111,y+31),radius=3,outline=c)
    text(im,(x+10,y+8),"Alder Reach",12,ink)
    d.line([(x+91,y+14),(x+95,y+18),(x+99,y+14)],fill=ink,width=1)
def control_sheet():
    out=Image.new("RGB",(1080,508),"#191a1b")
    for i,light in enumerate([False,True]):
        x=i*540; ink="#222526" if light else "#f0f0ed"; muted="#515658" if light else "#b9bcbb"
        bg="#f4f4f2" if light else "#202223"; surface="#e9eae7" if light else "#2b2d2e"
        d=ImageDraw.Draw(out);d.rectangle((x,0,x+539,508),fill=bg)
        text(out,(x+18,16),"Light" if light else "Dark",20,ink,True)
        text(out,(x+18,48),"Existing control",13,muted)
        if not light:
            out.paste(Image.open(P/"current.png").crop((984,111,1079,154)),(x+140,38))
        else: button(out,x+145,46,"Play",True,'base')
        for top,style,caption in [(106,'quiet','Raised fill'),(222,'tonal','Tonal step')]:
            text(out,(x+18,top),caption,16,ink,True)
            d.rectangle((x+18,top+28,x+521,top+91),fill=surface)
            sprite(out,'fox',x+30,top+45,32)
            text(out,(x+72,top+40),"Timberline Trail",16,ink,True);text(out,(x+72,top+64),"Local instance",12,muted)
            chooser(out,x+306,top+44,light);button(out,x+432,top+44,"Play",light,style)
        text(out,(x+18,336),"Dedicated server",16,ink,True)
        sprite(out,'beacon',x+25,366,32);text(out,(x+72,366),"North Crossing",16,ink,True);text(out,(x+72,389),"Bound server",12,muted)
        button(out,x+425,367,"Join",light)
        d.line((x+18,418,x+521,418),fill="#c1c3c1" if light else "#404344")
        for n,(label,pressed,focus) in enumerate([("Rest",False,False),("Pressed",True,False),("Keyboard focus",False,True)]):
            xx=x+23+n*165; text(out,(xx,434),label,12,muted);button(out,xx,457,"Play",light,pressed=pressed,focused=focus)
    out.save(P/"launch-options.png")

def diorama():
    im=Image.new("RGB",(200,72),"#292d2d");d=ImageDraw.Draw(im)
    for row in range(8):
        for col in range(15):
            x=35+col*7-row*7;y=5+col*2+row*3
            water=(col+2*row)%13 in (0,1,2)
            level=1 if water else 3+(col*3+row*2)%3
            y-=level
            d.polygon([(x,y),(x+7,y+3),(x,y+6),(x-7,y+3)],fill=("#526f78" if water else ["#778665","#8d9a76","#65785a"][(col+row)%3]))
            d.polygon([(x-7,y+3),(x,y+6),(x,y+6+level),(x-7,y+3+level)],fill="#475345")
            d.polygon([(x+7,y+3),(x,y+6),(x,y+6+level),(x+7,y+3+level)],fill="#38453e")
    return im
def world_sheet():
    out=Image.new("RGB",(1080,454),"#1b1d1e");d=ImageDraw.Draw(out)
    for col,(label,art) in enumerate([("Explored terrain",True),("Original diorama",False)]):
        x=col*540;text(out,(x+18,15),label,19,bold=True)
        src=Image.open(P/"alder-reach.png").crop((117,202,407,269)) if art else diorama().crop((0,0,140,62))
        cover=src.resize((500,125),Image.Resampling.NEAREST)
        out.paste(cover,(x+18,50))
        d.rectangle((x+18,175,x+517,260),fill="#292b2c")
        sprite(out,'fox',x+30,190,32);text(out,(x+75,187),"Timberline Trail",17,bold=True)
        text(out,(x+75,214),"Last played · Alder Reach",13,"#c0c3c0")
        for j,action in enumerate(['Pin','Map']):
            a=x+326+j*54;d.rounded_rectangle((a,190,a+47,220),3,outline="#606461");text(out,(a+9,198),action,12)
        button(out,x+433,190,"Play")
        text(out,(x+18,273),"Compact",13,"#c5c7c4")
        d.rectangle((x+18,295,x+517,351),fill="#292b2c")
        sprite(out,"gear",x+29,306,32)
        text(out,(x+73,303),"Redstone Workshop",15,bold=True);text(out,(x+73,326),"No world entered yet",12,"#bcbebc")
        button(out,x+433,306,"Play")
        text(out,(x+18,374),"Pinned shortcuts",13,"#c5c7c4")
        for j,name in enumerate(('fox','beacon','mooncap')):
            a=x+18+j*55;d.rounded_rectangle((a,397,a+43,440),3,fill="#2b2e2c",outline="#f0efeb" if j==0 else "#575b57",width=2 if j==0 else 1);sprite(out,name,a+6,403,32)
    out.save(P/"world-options.png")

if __name__=='__main__':
    surface_sheet();control_sheet();world_sheet()

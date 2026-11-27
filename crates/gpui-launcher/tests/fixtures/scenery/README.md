# Synthetic scenery package fixture

`movie.mp4` is a synthetic 320x180, 30 fps, 30-frame H.264 High clip: a red
band above a blue band with a moving white marker. The bands make a vertical
flip obvious; the marker makes adjacent frames differ. `movie.jpg` is its first
frame.

It is generated locally and contains no game or personal content. Regenerate
with FFmpeg 7:

```sh
ffmpeg -f lavfi -i "color=c=black:s=320x180:d=1:r=30" \
  -vf "drawbox=x=0:y=0:w=320:h=90:color=red:t=fill,drawbox=x=0:y=90:w=320:h=90:color=blue:t=fill,drawbox=x='mod(t*140,260)':y=80:w=60:h=20:color=white:t=fill" \
  -c:v libx264 -profile:v high -pix_fmt yuv420p -crf 18 -g 30 -bf 0 -frames:v 30 movie.mp4
ffmpeg -i movie.mp4 -frames:v 1 -q:v 2 movie.jpg
```

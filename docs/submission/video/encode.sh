#!/usr/bin/env bash
# Normalize recorded segments, compress the live wait (labelled with its real duration), join.
set -euo pipefail
cd /tmp/video

START=12.5   # seconds of the live wait kept in real time
SPEED=8
END="$(python3 -c "import json; print(json.load(open('seg3.times.json'))['tProgressEnd'])")"
TOTAL="$(ffprobe -v error -show_entries format=duration -of csv=p=0 seg3.webm)"
REAL="$(python3 -c "print(round($END - $START))")"

python3 - "$REAL" "$SPEED" <<'EOF'
import sys
from PIL import Image, ImageDraw, ImageFont
real, speed = sys.argv[1], sys.argv[2]
f = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", 22)
txt = f"SPED UP {speed}x   real time: {real} s"
w, h = 420, 46
im = Image.new("RGBA", (w, h), (11, 12, 10, 235))
d = ImageDraw.Draw(im)
d.rectangle([0, 0, w - 1, h - 1], outline=(224, 128, 90, 255), width=2)
d.text((16, 10), txt, font=f, fill=(232, 227, 210, 255))
im.save("label.png")
print("label:", txt)
EOF

N="fps=30,scale=1280:720,format=yuv420p,setsar=1"
enc() { ffmpeg -hide_banner -loglevel error -y "$@"; }
OUT=(-c:v libx264 -preset medium -crf 21 -pix_fmt yuv420p -r 30 -an)

enc -i seg1.webm -vf "$N" "${OUT[@]}" p1.mp4
enc -i seg2.webm -vf "$N" "${OUT[@]}" p2.mp4

enc -i seg3.webm -vf "trim=0:${START},setpts=PTS-STARTPTS,$N" "${OUT[@]}" p3a.mp4
enc -i seg3.webm -loop 1 -i label.png \
  -filter_complex "[0:v]trim=${START}:${END},setpts=(PTS-STARTPTS)/${SPEED},${N}[a];[a][1:v]overlay=W-w-30:30:shortest=1[v]" \
  -map "[v]" "${OUT[@]}" p3b.mp4
enc -i seg3.webm -vf "trim=${END}:${TOTAL},setpts=PTS-STARTPTS,$N" "${OUT[@]}" p3c.mp4

enc -i seg4.webm -vf "$N" "${OUT[@]}" p4.mp4

printf "file 'p1.mp4'\nfile 'p2.mp4'\nfile 'p3a.mp4'\nfile 'p3b.mp4'\nfile 'p3c.mp4'\nfile 'p4.mp4'\n" > list.txt
enc -f concat -safe 0 -i list.txt -c copy -movflags +faststart revenant-demo.mp4

for f in p1 p2 p3a p3b p3c p4 revenant-demo; do
  printf "%-14s %6.1fs\n" "$f" "$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$f.mp4")"
done

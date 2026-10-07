# The cards' faces

Fifty-two pictures, `c01.png` to `s13.png`: the suit's first letter and the
rank, ace low.

They are Byron Knoll's Vector Playing Cards, which he released into the
public domain (<https://code.google.com/archive/p/vector-playing-cards/>),
taken as the SVGs in <https://github.com/hayeah/playing-cards-assets>
(`svg-cards/`), which carries them unchanged under the same terms. Nothing
of that repository's own code is here.

Each was drawn at 300 by 436 with `rsvg-convert -w 300 -h 436 -b white`,
on Alpine 3.24 with `ttf-liberation` and `ttf-dejavu` installed for the
corner letters, and cut to 96 colours with `pngquant --strip --speed 1 96`.
The program scales them to the size it draws at.

# The cards' back

`back.png` was made for this program on 2026-10-07 by an image model,
FLUX.1 [schnell] (Apache-2.0), run on a machine of our own with kvad
0.14.1:

```
kvad images make "Full-bleed flat vector illustration, layered mountain
ridges fading into mist, deep teal and midnight blue with pale cyan haze,
thin fine white contour lines, a small crescent moon, calm, minimal,
elegant, symmetrical composition, edge to edge, no border, no frame, no
text, no letters, no symbols" --model black-forest-labs/FLUX.1-schnell
--size 832x1216 --seed 21
```

cut to a card's shape and scaled to 300 by 436. Nobody else's picture went
into it, and nothing restricts its use. It does not follow the theme: the
back painted where it cannot be decoded does.

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

# The cards' backs

Twelve pictures, `back-01.png` to `back-12.png`, in the order
`src/faces.rs` names them. Each was made for this program on 2026-10-07 by
an image model, FLUX.1 [schnell] (Apache-2.0), run on a machine of our own
with kvad 0.14.1:

```
kvad images make "PROMPT, edge to edge, full-bleed, no border, no frame,
no text, no letters, no numbers" --model black-forest-labs/FLUX.1-schnell
--size 832x1216 --seed SEED
```

then cut to a card's shape, scaled to 300 by 436 and cut to 128 colours.
Nobody else's picture went into any of them, and nothing restricts their
use. Wave is after Hokusai's print of 1831, long in the public domain; the
two retro ones are in the manner of the home computers of the 1980s and
show no maker's name or mark.

| | Name | Seed | What was asked for |
|---|---|---|---|
| 01 | Mist | 21 | Layered mountain ridges fading into mist, deep teal and midnight blue, a small crescent moon |
| 02 | Night | 3 | The same |
| 03 | Crimson | 7 | A classic playing card back, symmetrical filigree and scrollwork in crimson and white |
| 04 | Navy | 7 | A classic playing card back, a lattice of small diamonds and arabesques in navy and white |
| 05 | Aurora | 7 | Northern lights in green and violet over a dark fjord |
| 06 | Sunset | 7 | Layered mountain ridges at sunset, orange and coral fading to purple |
| 07 | Deco | 7 | Art deco sunburst fans in gold on black |
| 08 | Wave | 7 | An ukiyo-e woodblock print of great curling waves in indigo and cream |
| 09 | Neon | 12 | A 1980s synthwave grid running to a striped setting sun |
| 10 | Garden | 7 | A William Morris style wallpaper of leaves, vines and small flowers |
| 11 | 8-bit | 7 | Pixel art from a 1984 home computer: a castle on a hill under a moon, sixteen colours |
| 12 | 16-bit | 7 | 1987 demoscene art: a red and white checkered sphere over a magenta grid, copper bars behind |

They do not follow the theme: the plain back painted where one cannot be
decoded does.

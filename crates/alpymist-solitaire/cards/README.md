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

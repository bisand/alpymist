# Appearance
<!-- group: The desktop -->

Settings › Appearance has four things: the style, the accent colour, the text
size and the wallpaper.

## Style and accent

| Setting | Choices | Command |
|---|---|---|
| Style | Dark, Light | `alpymist set appearance.scheme light` |
| Accent colour | Mist, Fjord, Moss, Amber, Heather, Rose | `alpymist set appearance.accent moss` |

The style is light text on dark, or dark on light. The accent is the colour
of focus, selection and the main button. Together they reach the menu, the
popups, Settings and Alpymist's other windows, the top bar, the terminal,
window borders, notifications, and GTK applications. The bar, open terminals,
notifications and GTK applications change at once; Alpymist's own windows
that are already open keep what they had.

## Text size

`appearance.text-size`, from 12 to 24 pixels, is how large text is in the
menu, the popups and Alpymist's windows. The default is 16.

## Wallpaper

The page shows every picture as a thumbnail, with the one on the desktop
ringed. Click one and it is on the desktop at once.

```sh
alpymist set appearance.wallpaper milky-way.jpg
```

Alpymist ships eleven mountain pictures, seven photographs and four
paintings, and a new account starts on Blue hour.

### Your own pictures

Settings offers every picture in `/usr/share/backgrounds/alpymist/`. Copy one
there and it appears in the list:

```sh
doas cp ~/Pictures/harbour.jpg /usr/share/backgrounds/alpymist/
alpymist set appearance.wallpaper harbour.jpg
```

Pictures are drawn to fill the screen, so one that is 16:9 and at least as
large as your screen looks best. On a screen of another shape a strip is lost
off the sides rather than the picture being stretched.

## The picture at boot, login and lock

The boot menu, the splash, the login screen and the lock all show one
picture, Milky way, so that a boot has no cut in it until the desktop. That
one is not a setting.

## Going further

- Window gaps, rounding, borders and animations are Hyprland's, in
  `~/.config/hypr/hyprland.conf` under `general`, `decoration` and
  `animations`. Blur and shadows are off as shipped, because they are what
  makes a slow machine feel slow; turn them on there if yours is not.
- The terminal's font and colours beyond the theme are in
  `~/.config/foot/foot.ini`.
- The style, the accent and the fonts and their size are kept in
  `~/.config/alpymist/theme.toml`, which is where to name another font.

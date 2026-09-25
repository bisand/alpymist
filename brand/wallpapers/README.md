# Wallpapers

The pictures Alpymist ships. The `alpymist-wallpapers` package installs every
`*.jpg` here to `/usr/share/backgrounds/alpymist/`, and
`alpymist-splash-picture` installs its own copy of the one the boot, the login
screen and the lock show, as `/usr/share/alpymist/picture.jpg`.

| File | Picture | Used as |
| --- | --- | --- |
| `blue-hour.jpg` | Teal night sky over a green valley and its peaks. | The desktop, on every tier, for new accounts. |
| `milky-way.jpg` | The Milky Way over a dark ridge and a grassy crest. | Alpymist's own picture: the syslinux and GRUB menus, the splash, the login screen and the lock. |
| `morning-haze.jpg` | Blue ranges in haze under a peach sky, a wooded hill in front. | — |
| `red-sun.jpg` | A peak in silhouette against a low red sun. | — |
| `cloud-sea.jpg` | A lit summit above a sea of cloud. | — |
| `golden-slope.jpg` | A sunset between steep ridges, over an orange meadow. | — |
| `last-light.jpg` | The sun setting between twin peaks, a snowfield below. | — |
| `green-hill.jpg` | Painted: blue peaks and a green hill under a yellow sky. | — |
| `sun-crown.jpg` | Painted: a sun behind twin snowy peaks, mist in the valley. | — |
| `spiral-sun.jpg` | Painted: a swirling orange sun over glowing ridges. | — |
| `star-peak.jpg` | Painted: a white peak under a deep blue, starry sky. | — |

The desktop default is set in each tier's skel: `$wallpaper` in
`desktop/skel/full/.config/hypr/hyprland.conf`, the `swaybg` line in
`desktop/skel/lite/.config/labwc/autostart`, and the `feh` line in
`desktop/skel/legacy/.config/i3/config`. Changing it reaches new accounts only.
Their home directories are theirs. Alpymist's own picture is `_picture` in
`aports/alpymist-splash/APKBUILD`, and nothing else names it.

`alpymist.png`, the drawn mountains, is not here: `alpymist-desktop` renders it
at build time, and it stays installed because every account made before these
existed points at it.

## What a wallpaper here has to be

- **16:9, 1920x1080.** It is the most common panel shape, and every desktop
  here draws in fill mode, so a 16:10 or 4:3 screen loses a strip off the sides
  and nothing is stretched or letterboxed. Keep what matters out of the outer
  tenth on each side.
- **JPEG, quality 92, no chroma subsampling.** GRUB on arm64 reads nothing
  else, and gdk-pixbuf (for `swaybg`) and imlib2 (for `feh`) both read it. At
  this quality the dark skies show no banding even with the contrast pushed
  hard, at a tenth of the PNG's size.
- **Nothing but the picture.** No taskbars, icons, clocks or signatures.
- **Named for what is in it**, lower case with hyphens.

## Where these came from

All eleven were generated as 1216x832 renders named `kvad-8` to `kvad-18`,
several with a made-up desktop painted into them. The originals are not in
the repository. Each was cropped to 16:9 clear of that desktop chrome, then
scaled to 1920x1080 with a Lanczos filter:

| File | Source | Crop (w×h+x+y) | Removed |
| --- | --- | --- | --- |
| `morning-haze.jpg` | kvad-8 | 1216×684+0+104 | top bar, taskbar |
| `red-sun.jpg` | kvad-9 | 1216×684+0+106 | top bar, taskbar |
| `cloud-sea.jpg` | kvad-10 | 1216×684+0+100 | taskbar |
| `golden-slope.jpg` | kvad-11 | 1168×657+48+104 | top bar, desktop icons, taskbar |
| `last-light.jpg` | kvad-12 | 1216×684+0+108 | desktop icon, taskbar |
| `green-hill.jpg` | kvad-13 | 1216×684+0+74 | — |
| `sun-crown.jpg` | kvad-14 | 1216×684+0+90 | — |
| `spiral-sun.jpg` | kvad-15 | 1216×684+0+0 | signature |
| `star-peak.jpg` | kvad-16 | 1216×684+0+60 | — |
| `milky-way.jpg` | kvad-17 | 1216×684+0+100 | — |
| `blue-hour.jpg` | kvad-18 | 1216×684+0+100 | — |

That is an upscale of about 1.6×, so these are softer at 1080p than a native
picture would be. A larger source should be cropped first and scaled down to
1920x1080 instead.

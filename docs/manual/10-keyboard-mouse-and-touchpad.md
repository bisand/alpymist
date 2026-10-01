# Keyboard, mouse and touchpad
<!-- group: The desktop -->

## Keyboard

| Setting | Meaning | Command |
|---|---|---|
| Keyboard layout | The layout the desktop, the login screen and the console all type with. For everyone on the computer. | `alpymist set keyboard.layout no` |
| Repeat delay | How long a key is held before it repeats: 150 to 1000 ms, 600 by default | `alpymist set keyboard.repeat-delay 400` |
| Repeat rate | Repeats a second: 10 to 80, 25 by default | `alpymist set keyboard.repeat-rate 40` |

`alpymist list keyboard.layout` prints every layout there is. The layout is
the one chosen in the installer until it is changed here, and changing it
asks for an administrator's password, since it changes the login screen too.

## Touchpad

| Setting | Default | Meaning |
|---|---|---|
| Natural scrolling | Off | Content moves the way your fingers do, as on a phone |
| Tap to click | On | A tap clicks, without pressing the pad down |
| Ignore while typing | On | The touchpad does nothing for a moment after a key is pressed |
| Scrolling speed | 100 % | How far content moves for a two-finger swipe: 25 to 300 % |

```sh
alpymist set touchpad.natural-scroll true
alpymist set touchpad.scroll-speed 150
```

## Mouse and pointer

| Setting | Default | Meaning |
|---|---|---|
| Pointer speed | 0 | From -100 to 100, for mice and touchpads alike |
| Pointer acceleration | Adaptive | Adaptive carries the pointer further for faster movements; flat counts every movement the same |
| Natural scrolling for mice | Off | The wheel moves content the other way round |
| Left-handed buttons | Off | Swaps the left and right buttons |

```sh
alpymist set mouse.acceleration flat
alpymist set mouse.left-handed true
```

All of these apply at once.

## Anything else

Hyprland can do more with input than Settings offers: per-device rules,
key remapping through XKB options, gestures. Those go in the `input` section
of your own `~/.config/hypr/hyprland.conf`, which is read after Alpymist's
settings, so a line of yours wins.

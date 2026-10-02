# Screens
<!-- group: The desktop -->

Plug in a screen and it is used: placed to the right of what is there, at its
own best resolution. Arrange it once, and Alpymist remembers that
arrangement for that set of screens. The desk's dock, the meeting room's
projector and the laptop alone each come back as you left them.

## Arranging screens

Settings › Displays draws the screens to scale.

- **Drag** a screen to where it stands on your desk. It lands against the
  nearest edge of another, so there are no gaps and no overlaps.
- With the keyboard, the **arrows** move the chosen screen beside another,
  and <kbd>Shift</kbd> and the arrows slide it along that edge a little at a
  time, for a screen that sits lower than its neighbour.
- **Identify** puts each screen's number on it.

Below the picture are the chosen screen's own settings:

| Setting | Meaning |
|---|---|
| Show things on it | Off leaves the screen connected and dark |
| Picture | Its own, or the same as another screen: a projector showing the laptop's |
| Resolution | Size and refresh rate |
| Scale | How much larger everything is drawn, from 100 % to 300 % |
| Rotation | Turned by 90, 180 or 270 degrees |

Nothing changes until **Apply**. The page then asks whether to keep the
layout, and if nobody answers in 15 seconds it puts the previous one back, so
a resolution the screen cannot show never leaves you without a picture.

## Workspaces on each screen

Each screen has its own workspaces 1 to 9. <kbd>Super</kbd>+<kbd>1</kbd> to
<kbd>9</kbd> switch between the workspaces of the screen the pointer is on,
and each screen's bar shows its own.

<kbd>Super</kbd>+<kbd>Shift</kbd> and an arrow move the window with the focus
that way among the others, and from the edge of a screen on to the screen on
that side, onto the workspace it is showing. An account made before these keys
is given them the next time either setting below is changed, unless it already
uses one of the four for something else. One given them when they only took a
window to another screen has them put right when it next logs in.

<kbd>Super</kbd>+<kbd>Alt</kbd> and an arrow take the window straight to the
screen on that side, floating or not, in one press, however many windows are
between it and the edge. Every account has these from the package.

<kbd>Super</kbd>+<kbd>Tab</kbd> shows the nine workspaces of the screen the
pointer is on, on that screen; move the pointer to another screen and press it
again for that one's. An account made before the key is given it the same way,
unless it already uses <kbd>Super</kbd>+<kbd>Tab</kbd>. The overview is loaded
when the desktop starts, so the first time after the upgrade that brought it,
log out and in.

When a screen goes away its workspaces move to the screens that are left, and
when it comes back they go home.

*Each screen has its own workspaces*, on that page, turns this off: then
there is one set of nine shared by every screen, as Hyprland has it.

## The lid

With another screen on, closing a laptop's lid turns its own screen off and
moves what was on it to the others; opening it puts everything back. *Turn
the laptop's screen off when the lid closes* is the switch for that.

What the lid does to the machine, suspend or nothing, is a separate setting
in [Power](13-power.md), and with a screen connected it is *nothing* by
default, so a docked laptop keeps running when closed.

The login screen follows the lid too: with the lid closed it appears on the
external screen.

## From the command line

```sh
alpymist displays list
alpymist displays set DP-3 --mode 1920x1080@60 --position 1920,0
alpymist displays set S24E650 --scale 1.25 --rotate 90
alpymist displays set HDMI-A-1 --mirror eDP-1
alpymist displays set HDMI-A-1 --mirror none
alpymist displays set DP-5 --on false
alpymist displays save      # remember the screens as they are now
alpymist displays apply     # put the remembered layout in place
alpymist displays forget    # forget it: the screens are extended again
alpymist displays overview  # what Super+Tab does
```

A screen is named by its connector, as `list` shows it, or by part of its
model name. Layouts are kept by make, model and serial number, so a screen
keeps its place whichever port it is plugged into.

## Where it is kept

Layouts are in `~/.config/alpymist/displays.toml`. From them Alpymist writes
Hyprland monitor rules to `~/.config/alpymist/hypr/displays.conf`, which is
rewritten whenever the screens change: change a layout with Settings or
`alpymist displays`, not by editing that file. A `monitor =` line in your own
`hyprland.conf` still applies to a screen Alpymist's rules do not name.

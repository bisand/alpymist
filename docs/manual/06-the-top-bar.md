# The top bar
<!-- group: The desktop -->

The bar runs along the top of every screen. Most of what is on it opens
something when clicked.

## Left

| Item | Shows | Click |
|---|---|---|
| The Alpymist mark | | Opens the menu |
| Workspaces | The first five of this screen, and any others in use. The one showing is marked. | Goes to that workspace |

## Middle

The clock. Resting the pointer on it shows a calendar, and a click switches
to the long date and back. It is a 24-hour clock unless Settings › Date &
time says otherwise.

## Right

| Item | Shows | Click |
|---|---|---|
| The arrow | Applications running in the background, which slide out when the pointer is on it | Whatever the application's own icon does |
| AI | How much of your AI limits is used, once a provider is set up | Opens the AI usage popup. A right click asks again now. |
| Bluetooth | What is connected. Absent while Bluetooth is off. | Opens the Bluetooth device list in a terminal |
| Wi-Fi | The network and its strength | Opens the Wi-Fi popup |
| Network | A wired connection, when there is one | |
| Speaker | The volume | Opens the volume mixer. A right click mutes. |
| Processor | Use, when the pointer rests on it | |
| Memory | Use, when the pointer rests on it | |
| Battery | The charge, and whatever else Settings › Power adds | Opens the power popup |
| Power | | Opens the System menu |

The Wi-Fi, power and AI popups open under the bar and close with
<kbd>Esc</kbd> or a second click.

## Hiding it

Toggle › Top bar in the menu takes the bar away and brings it back.

## Changing it

The bar is Waybar. Your own file, `~/.config/waybar/hyprland.jsonc`, starts
as one line that includes Alpymist's:

```json
{
  "include": ["/usr/share/alpymist/waybar/hyprland.jsonc"]
}
```

Anything you add beside that line overrides the packaged bar, and the rest
stays current through upgrades. To put the clock on the right, for instance,
set `modules-center` and `modules-right` there. Setup › Config › Top bar opens
the file, and Top bar style opens `~/.config/waybar/style.css`. Save, then
restart the bar to see the change: Toggle › Top bar in the menu, twice.

The bar's colours follow the theme chosen in Settings › Appearance.

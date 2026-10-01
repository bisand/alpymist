# Notifications
<!-- group: The desktop -->

Notifications appear in a corner of the screen and go away by themselves.

| Setting | Choices | Default |
|---|---|---|
| Do not disturb | On or off | Off |
| Position | Top right, top centre, top left, bottom right, bottom centre, bottom left | Top right |
| Show for | 3, 6, 10, 15 or 30 seconds, or until dismissed | 6 seconds |

```sh
alpymist set notifications.do-not-disturb true
alpymist set notifications.position bottom-right
alpymist set notifications.timeout 10
```

*Show for* applies when the program that sent the notification does not say
how long it should stay.

## Do not disturb

With it on, no new notification is shown until it is turned off again, or
until you log out. Toggle › Do not disturb in the menu is the quick way.

## Going further

Notifications are drawn by `mako`, and `~/.config/mako/config` is written by
Settings from the three settings above and the theme. If you edit that file
by hand, Alpymist keeps it as you left it, and Settings › Notifications says
that it can no longer change it.

`makoctl` works as usual: `makoctl dismiss -a` dismisses everything showing,
and `makoctl restore` brings the last one back.

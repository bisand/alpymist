# Power
<!-- group: The desktop -->

Click the battery on the bar, or open Setup › Power in the menu, for the
power popup: the charge, the power mode, and what the lid and the power
button do. Settings › Power has the same and a little more.

## Power mode

| Mode | Meaning |
|---|---|
| Power saver | Longest on a battery |
| Balanced | The default |
| Performance | Gives the processor everything |

```sh
alpymist-power profile                # the mode, and the modes offered
alpymist-power profile power-saver
alpymist set power.mode performance
```

## The lid and the power button

| Setting | Choices | Default |
|---|---|---|
| Closing the lid, on battery | Suspend, lock, nothing, hibernate | Suspend |
| Closing the lid, on the charger | The same | Suspend |
| Closing the lid, with a screen connected | The same | Nothing |
| Power button | The System menu, suspend, power off, nothing | The System menu |

Suspending always locks the screen first. The popup offers hibernate only on
a machine that can do it; chosen anyway, a lid that cannot hibernate locks
instead.

```sh
alpymist set power.lid lock
alpymist set power.lid-on-power nothing
alpymist set power.button suspend
```

## Charge limit

A battery that is mostly plugged in lasts longer if it is not kept full.
*Charge limit* stops charging at 90, 80 or 60 %, on laptops whose firmware
allows it. Changing it asks for an administrator's password.

```sh
alpymist set power.charge-limit 80
```

## What the bar shows

Beside the battery icon, any of these, each a switch in Settings › Power:

| Setting | Shows | Default |
|---|---|---|
| Battery percentage in the bar | The charge | On |
| Time left in the bar | How long the battery lasts, or until it is full | Off |
| Power draw in the bar | Watts in or out | Off |
| Power mode in the bar | The mode | Off |

## Suspending, restarting, shutting down

From the System menu on <kbd>Super</kbd>+<kbd>Esc</kbd>, or:

```sh
alpymist-power suspend
alpymist-power hibernate
alpymist-power restart
alpymist-power power-off
alpymist-power status
```

None of these asks for a password for someone sitting at the machine.

## When the screen turns off by itself

That is idleness, not power, and is set in
[Screensaver, idle and lock](14-screensaver-idle-and-lock.md).

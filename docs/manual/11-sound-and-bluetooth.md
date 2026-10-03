# Sound and Bluetooth
<!-- group: The desktop -->

## Sound

Sound is PipeWire. It needs no setting up: plug in headphones and they are
used.

Settings › Sound has the output device and its volume, and the input device
and its volume. *Automatic* leaves the choice of device to the system, which
prefers what it rates best of what is connected. A dock's sound comes after
the laptop's own speakers there: a dock cannot tell whether anything is
plugged into its jack, and sound sent to an empty one is never heard. A
device you choose, the dock's included, is remembered through restarts and
used whenever it is connected.

```sh
alpymist set sound.volume 60
alpymist set sound.input-volume 80
alpymist list sound.output      # the devices there are
```

The keyboard's volume, mute and microphone keys work wherever a keyboard
has them, on the lock screen too, and the play, next and previous keys
control whatever is playing; see [Hotkeys](04-hotkeys.md#the-control-keys).

On the bar, a click on the speaker opens the volume mixer (pavucontrol), with
a volume for each program that is playing and each device. A right click
mutes. Setup › Audio in the menu opens the same mixer.

## Bluetooth

Bluetooth is installed and **off** until you turn it on:

```sh
alpymist set bluetooth.enabled true
```

or Settings › Bluetooth. It is a system setting, so it asks for an
administrator's password, and it stays on at every start from then on. Other
devices cannot see the computer unless you make it discoverable while
pairing.

To pair, connect and forget devices, open Setup › Bluetooth in the menu or
click the Bluetooth icon on the bar. That opens `bluetuith`, a device list in
a terminal: scan, choose a device, pair and connect.

Bluetooth headphones and speakers then appear as sound devices like any
other.

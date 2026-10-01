# Networking
<!-- group: The desktop -->

## Wi-Fi

Click the Wi-Fi icon on the bar, or open Setup › Wi-Fi in the menu. The popup
lists the networks in range. Choose one, type its password, and it is
remembered and joined again whenever it is in range.

```sh
alpymist-wifi status      # where Wi-Fi is
alpymist-wifi scan        # look for networks and list them
alpymist-wifi off         # the radio off; on, or toggle
alpymist set wifi.enabled false
```

Wi-Fi is `iwd`. Its own tools are there for what the popup does not do:
`iwctl` on the command line, and `impala`, a terminal interface.

## Wired

A wired connection is used as soon as the cable is in. The bar shows it, with
its address when the pointer rests on it.

## The SSH server

The SSH server is off, and not even installed, until you turn it on:

```sh
alpymist set ssh.server on
```

or Settings › SSH, or Toggle › SSH server in the menu. It lets the accounts
on this computer log in to it from another one over the network, so it asks
for an administrator's password. Turning it off stops it again.

Connecting *from* this computer needs nothing turned on: `ssh` is installed.
See [Security](22-security.md) for how key passphrases are asked for and
forgotten.

## Firmware for network cards

Some Wi-Fi and network cards need firmware files that are not installed by
default. A service watches for a driver asking for one and installs the
package that has it, so a card that did not work at first boot may work after
the next one with a network connection. `doas alpymist firmware check` does
that once, now.

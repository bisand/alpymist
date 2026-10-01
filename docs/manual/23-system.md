# Accounts, language and time
<!-- group: System -->

## Your account

Settings › System has what belongs to the computer and to your account on it.

| Setting | Meaning |
|---|---|
| Computer name | What the computer is called on the network, in the terminal's prompt and on the login screen |
| Password | Changes your password, in a terminal that asks for the old one first |
| Administrator | Whether this account may change settings for everyone and install software. The last administrator cannot be turned off. |
| Unlock with a fingerprint | See [Screensaver, idle and lock](14-screensaver-idle-and-lock.md) |

```sh
alpymist set system.hostname workbench
passwd
```

## More people

The login screen lists every account on the machine, so adding one is all it
takes. There is no page for that yet; from a terminal:

```sh
doas adduser -s /bin/zsh -g "Kari Nordmann" kari
for group in seat video audio netdev; do doas adduser kari $group; done
```

The first line makes the account with the shell the desktop sets up, asks
for its password, and gives it the desktop's configuration. The second puts it in the groups the installer puts
the first account in; without `seat` the desktop cannot open the screen and
keyboard, and the login goes straight back to the login screen.

To make the account an administrator as well, `doas adduser kari wheel`, or
turn on *Administrator* in Settings › System while logged in as it.

`doas deluser kari` removes an account, and `doas deluser --remove-home kari`
its files with it.

## Language

| Setting | Meaning |
|---|---|
| Language | The language programs speak, and how they write dates and numbers. Yours alone. |
| Translations | Installs every program's translations, about 30 MB, so programs can speak that language |

```sh
alpymist list language.language      # the choices
alpymist set language.language nb_NO.UTF-8
alpymist set language.translations true
```

Both apply at the next login. Alpymist's own windows are in English.

## Date and time

| Setting | Meaning |
|---|---|
| Time zone | For everyone on the computer. Chosen in the installer. |
| Set the time from the network | Keeps the clock right by asking time servers. The installer turns it on, so `alpymist reset` on it turns it off. |
| 24-hour clock | 14:30 rather than 2:30 PM, in the bar and on the login and lock screens |

```sh
alpymist set datetime.timezone Europe/Oslo
alpymist set datetime.24-hour false
```

Leave network time on. A clock that is wrong by years, as after a flat
battery, makes every secure connection fail, and updates and the Store with
them, with nothing on screen to say why.

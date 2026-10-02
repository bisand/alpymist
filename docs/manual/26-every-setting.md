# Every setting
<!-- group: Reference -->

Every setting Alpymist has, by area. Each can be changed in the Settings app,
found by its title in the menu's search, or read and changed from a terminal:

```sh
alpymist get power.lid
alpymist set power.lid lock
alpymist reset power.lid
alpymist list power.lid      # its value, and what it can be
```

**System** marks a setting that changes the machine for everyone and asks for
an administrator's password; the rest are your account's own. Where a change
does not apply at once, the setting says when.

This page is made from the settings themselves, and a test keeps it so: it
is what `alpymist list` prints on a system with nothing added. Yours also
lists the settings that packages you installed have added.

## Appearance

| Setting | Values | Default |
|---|---|---|
| **Style**<br>Light text on dark, or dark text on light.<br>`appearance.scheme` · New windows | `dark`, `light` | `dark` |
| **Accent colour**<br>The colour of focus, selection and the main button.<br>`appearance.accent` · New windows | `mist`, `fjord`, `moss`, `amber`, `heather`, `rose` | `mist` |
| **Text size**<br>How large text is in the menu, the popups and Alpymist's windows.<br>`appearance.text-size` · New windows | 12 to 24 px | `16` |
| **Wallpaper**<br>The picture behind the desktop.<br>`appearance.wallpaper` | The pictures installed | `blue-hour.jpg` |

## Displays

| Setting | Values | Default |
|---|---|---|
| **Each screen has its own workspaces**<br>Super+1 to Super+9 switch between the workspaces of the screen the pointer is on. Off, workspaces 1 to 9 are shared by every screen.<br>`displays.workspaces` | `true`, `false` | `true` |
| **Turn the laptop's screen off when the lid closes**<br>While another screen is on, closing the lid turns the built-in screen off and moves what was on it to the others.<br>`displays.lid` | `true`, `false` | `true` |

## Keyboard

| Setting | Values | Default |
|---|---|---|
| **Keyboard layout**<br>The layout every desktop, the login screen and the console type with.<br>`keyboard.layout` · System | Every XKB layout | `us` |
| **Repeat delay**<br>How long a key is held before it starts repeating.<br>`keyboard.repeat-delay` | 150 to 1000 ms | `600` |
| **Repeat rate**<br>How many times a second a held key repeats.<br>`keyboard.repeat-rate` | 10 to 80 per second | `25` |

## Language

| Setting | Values | Default |
|---|---|---|
| **Language**<br>The language programs speak, and how they write dates and numbers.<br>`language.language` · Next login | The languages there are | `C.UTF-8` |
| **Translations**<br>Install every program's translations, about 30 MB, so programs can speak the language above.<br>`language.translations` · System · Next login | `true`, `false` | `false` |

## Touchpad

| Setting | Values | Default |
|---|---|---|
| **Natural scrolling**<br>Content moves the way your fingers do, as on a phone.<br>`touchpad.natural-scroll` | `true`, `false` | `false` |
| **Tap to click**<br>A tap on the touchpad clicks, without pressing it down.<br>`touchpad.tap-to-click` | `true`, `false` | `true` |
| **Ignore while typing**<br>The touchpad does nothing for a moment after a key is pressed.<br>`touchpad.disable-while-typing` | `true`, `false` | `true` |
| **Scrolling speed**<br>How far content moves for a two-finger swipe.<br>`touchpad.scroll-speed` | 25 to 300 % | `100` |

## Mouse and pointer

| Setting | Values | Default |
|---|---|---|
| **Pointer speed**<br>How fast the pointer moves, for mice and touchpads alike.<br>`mouse.speed` | -100 to 100 | `0` |
| **Pointer acceleration**<br>Faster movements carry the pointer further, or every movement counts the same.<br>`mouse.acceleration` | `adaptive`, `flat` | `adaptive` |
| **Natural scrolling for mice**<br>The wheel moves content the other way round.<br>`mouse.natural-scroll` | `true`, `false` | `false` |
| **Left-handed buttons**<br>Swap the left and right buttons.<br>`mouse.left-handed` | `true`, `false` | `false` |

## Sound

| Setting | Values | Default |
|---|---|---|
| **Output device**<br>Where sound plays. Automatic leaves it to the system, which prefers what it rates best of what is connected.<br>`sound.output` | Automatic, or a device | `auto` |
| **Output volume**<br>How loud the output device plays.<br>`sound.volume` | 0 to 100 % | `40` |
| **Input device**<br>Which microphone records. Automatic leaves it to the system.<br>`sound.input` | Automatic, or a device | `auto` |
| **Input volume**<br>How much the input device picks up.<br>`sound.input-volume` | 0 to 100 % | `100` |

## Wi-Fi

| Setting | Values | Default |
|---|---|---|
| **Wi-Fi**<br>Turn the wireless adapter on or off.<br>`wifi.enabled` | `true`, `false` | `true` |

## Bluetooth

| Setting | Values | Default |
|---|---|---|
| **Bluetooth**<br>Turn Bluetooth on, now and at every start, to use headphones, keyboards and other devices. Other devices cannot see this computer unless you make it discoverable while pairing.<br>`bluetooth.enabled` · System | `true`, `false` | `false` |

## SSH

| Setting | Values | Default |
|---|---|---|
| **SSH server**<br>Let the accounts on this computer log in to it from another one, over the network. Installed the first time it is turned on.<br>`ssh.server` · System | `true`, `false` | `false` |

## Power

| Setting | Values | Default |
|---|---|---|
| **Closing the lid**<br>What happens when the lid closes on battery.<br>`power.lid` | `suspend`, `lock`, `nothing`, `hibernate` | `suspend` |
| **Closing the lid on the charger**<br>What happens when the lid closes while plugged in.<br>`power.lid-on-power` | `suspend`, `lock`, `nothing`, `hibernate` | `suspend` |
| **Closing the lid with a screen connected**<br>What happens when the lid closes with another screen plugged in.<br>`power.lid-docked` | `suspend`, `lock`, `nothing`, `hibernate` | `nothing` |
| **Power button**<br>What pressing the power button does.<br>`power.button` | `menu`, `suspend`, `power-off`, `nothing` | `menu` |
| **Battery percentage in the bar**<br>Show the charge beside the battery icon.<br>`power.bar-percentage` | `true`, `false` | `true` |
| **Time left in the bar**<br>Show how long the battery lasts, or until it is full.<br>`power.bar-time` | `true`, `false` | `false` |
| **Power draw in the bar**<br>Show how many watts go in or out.<br>`power.bar-power` | `true`, `false` | `false` |
| **Power mode in the bar**<br>Show the power mode beside the battery icon.<br>`power.bar-profile` | `true`, `false` | `false` |
| **Power mode**<br>Save battery, balance, or give the processor everything.<br>`power.mode` | `power-saver`, `balanced`, `performance` | `balanced` |
| **Charge limit**<br>Stop charging below full, which keeps a battery that is mostly plugged in healthier.<br>`power.charge-limit` · Asks for an administrator's password | `100`, `90`, `80`, `60` | `100` |

## Notifications

| Setting | Values | Default |
|---|---|---|
| **Do not disturb**<br>Show no new notifications until it is turned off, or until you log out.<br>`notifications.do-not-disturb` | `true`, `false` | `false` |
| **Position**<br>Which corner or edge of the screen notifications appear at.<br>`notifications.position` | `top-right`, `top-center`, `top-left`, `bottom-right`, `bottom-center`, `bottom-left` | `top-right` |
| **Show for**<br>How long a notification stays, when the program that sent it does not say.<br>`notifications.timeout` | `3`, `6`, `10`, `15`, `30`, `never` | `6` |

## AI usage

Each provider installed has a switch, `ai.provider-<id>`; these are the ones
shipped.

| Setting | Values | Default |
|---|---|---|
| **Anthropic API**<br>What the organisation's API use has cost this month<br>`ai.provider-anthropic-api` | `true`, `false` | `false` |
| **Claude**<br>A Pro or Max subscription's 5-hour and weekly limits, as fresh as Claude Code's last use. Needs Claude Code, Anthropic's command-line tool<br>`ai.provider-claude` | `true`, `false` | `false` |
| **ChatGPT (Codex)**<br>A ChatGPT plan's 5-hour and weekly Codex limits. Read in a way its vendor does not document: it may stop working. Needs Codex, OpenAI's command-line tool<br>`ai.provider-codex` | `true`, `false` | `false` |
| **Gemini CLI**<br>How much of each Gemini model's allowance is used, as fresh as Gemini CLI's last use. Read in a way its vendor does not document: it may stop working. Needs Gemini CLI, Google's command-line tool<br>`ai.provider-gemini` | `true`, `false` | `false` |
| **GitHub Copilot**<br>The month's premium requests, what is left of them, and when they start again. Read in a way its vendor does not document: it may stop working<br>`ai.provider-github-copilot` | `true`, `false` | `false` |
| **OpenAI API**<br>What the organisation's API use has cost this month<br>`ai.provider-openai-api` | `true`, `false` | `false` |
| **OpenRouter**<br>What this key has spent this month, its limit, and the credits left<br>`ai.provider-openrouter` | `true`, `false` | `false` |
| **Warn from**<br>How much of a limit is used before the bar changes colour.<br>`ai.warn-at` | 50 to 95 % | `80` |
| **Notify when a limit is near**<br>A notification when a provider passes that, and one more when it is nearly used up.<br>`ai.notify` | `true`, `false` | `true` |
| **Ask every**<br>Minutes between askings. A provider that allows less often is asked less often, and none is asked while the screen is locked.<br>`ai.refresh` | 1 to 120 min | `10` |
| **Stop asking on battery below**<br>On battery with less than this left, nothing is asked and the last answers stay. Zero asks regardless.<br>`ai.battery-floor` | 0 to 100 % | `20` |

## Clipboard

| Setting | Values | Default |
|---|---|---|
| **Keep a history**<br>Keep what you copy, to paste again from Super+Shift+V. Passwords and keys you copy are kept too, unless a password manager marks them secret.<br>`clipboard.history` | `true`, `false` | `false` |
| **Keep it after logging out**<br>Keep the history in a file only you can read, rather than in memory only.<br>`clipboard.remember` | `true`, `false` | `false` |
| **Entries kept**<br>The most the history keeps; the oldest go first. Pinned entries are not counted, and never go.<br>`clipboard.size` | 10 to 500 | `50` |
| **Forget it when the screen locks**<br>Forget all but pinned entries whenever the screen locks.<br>`clipboard.clear-on-lock` | `true`, `false` | `true` |
| **Clear the history**<br>Forget everything but pinned entries, now.<br>`clipboard.clear` | An action |  |

## Default applications

| Setting | Values | Default |
|---|---|---|
| **Web browser**<br>Opens links and web pages.<br>`default.browser` | The applications that open these |  |
| **Mail**<br>Opens email addresses and messages.<br>`default.mail` | The applications that open these |  |
| **File manager**<br>Opens folders.<br>`default.files` | The applications that open these |  |
| **Text editor**<br>Opens text files, configuration and code.<br>`default.editor` | The applications that open these |  |
| **Terminal**<br>Runs commands, and the programs that run in one.<br>`default.terminal` | The applications that open these |  |
| **Images**<br>Opens pictures and photos.<br>`default.images` | The applications that open these |  |
| **PDF**<br>Opens PDF documents.<br>`default.pdf` | The applications that open these |  |
| **Video**<br>Plays films and clips.<br>`default.video` | The applications that open these |  |
| **Music**<br>Plays songs and sound files.<br>`default.music` | The applications that open these |  |
| **Archives**<br>Opens zip files, tarballs and other archives.<br>`default.archives` | The applications that open these |  |
| **Calendar**<br>Opens calendar files and subscriptions.<br>`default.calendar` | The applications that open these |  |

## Startup

Each program that a package installs to start at login also has a switch
here, named `startup.<program>`.

| Setting | Values | Default |
|---|---|---|
| **Add a program**<br>Start an installed application every time you log in.<br>`startup.add` · Next login | The applications installed |  |

## Screensaver

| Setting | Values | Default |
|---|---|---|
| **Screensaver**<br>Which one appears, or a different one each time.<br>`screensaver.show` | `mountains`, `starfield`, `random` | `random` |
| **Show the screensaver after**<br>Minutes of stillness before it appears. Zero never shows it.<br>`screensaver.after` | 0 to 60 min | `5` |
| **Turn the screen off after**<br>Minutes of stillness before the screen turns off, counted from the last key or click. Zero leaves it on.<br>`screensaver.blank-after` | 0 to 60 min | `10` |
| **Main screen**<br>Where the screensaver shows while the others go dark. A screen that is not connected leaves it on the first.<br>`screensaver.main-screen` | The screens connected |  |
| **Show it on every screen**<br>Off, it shows on the main screen and the others go dark.<br>`screensaver.every-screen` | `true`, `false` | `false` |
| **Lock when the screen turns off**<br>Ask for the password to get back in.<br>`screensaver.lock` | `true`, `false` | `false` |

## Screensaver: Mountains

| Setting | Values | Default |
|---|---|---|
| **See Mountains now**<br>Show it until a key is pressed or the pointer moves.<br>`screensaver-mountains.preview` | An action |  |
| **How chunky the picture is**<br>Physical pixels to one drawn pixel. One is no pixelation at all.<br>`screensaver-mountains.block` | 1 to 16 px | `6` |
| **How much mist**<br>Thicker mist hides more of the ranges behind it.<br>`screensaver-mountains.mist` | 0 to 200 % | `100` |
| **How fast the ranges travel**<br>As a percentage of its usual pace. The nearest range travels fastest and every range behind it is close to twice as slow again — nine to one from the front of the picture to the back, which is what gives it depth rather than flatness.<br>`screensaver-mountains.speed` | 0 to 300 % | `100` |
| **How smoothly it scrolls**<br>Frames a second. The ranges travel in a straight line, so fewer frames read as the skyline jumping rather than as slowness — and every frame costs a little battery.<br>`screensaver-mountains.fps` | 8 to 30 fps | `12` |
| **An aircraft crossing**<br>An aeroplane comes in from the distance and goes back out again, with its navigation lights on and its strobe going. It passes behind the mountains.<br>`screensaver-mountains.aircraft` | `true`, `false` | `true` |
| **A balloon, now and then**<br>Every few minutes a hot-air balloon drifts through, with its burner flaring. It is the shape the Commodore 64 manual taught everybody to draw.<br>`screensaver-mountains.balloon` | `true`, `false` | `true` |

## Screensaver: Starfield

| Setting | Values | Default |
|---|---|---|
| **See Starfield now**<br>Show it until a key is pressed or the pointer moves.<br>`screensaver-starfield.preview` | An action |  |
| **How chunky the picture is**<br>Physical pixels to one drawn pixel. One is no pixelation at all.<br>`screensaver-starfield.block` | 1 to 16 px | `4` |
| **How smoothly it moves**<br>Frames a second. Stars travel in straight lines, so fewer frames read as judder rather than as slowness — and every frame costs a little battery.<br>`screensaver-starfield.fps` | 8 to 30 fps | `20` |
| **How fast the ship goes**<br>As a percentage of its usual cruise.<br>`screensaver-starfield.speed` | 25 to 300 % | `100` |
| **How many stars**<br>As a percentage of what a screen this size suggests.<br>`screensaver-starfield.stars` | 20 to 300 % | `100` |
| **Asteroid fields**<br>Every minute or so, a field of rocks with a gap through it that the ship steers for.<br>`screensaver-starfield.rocks` | `true`, `false` | `true` |
| **How chunky the rocks are**<br>Cells across a rock. Fewer is coarser: the rocks are drawn as square blocks however smoothly they move.<br>`screensaver-starfield.grain` | 6 to 24 | `12` |

## Date and time

| Setting | Values | Default |
|---|---|---|
| **Time zone**<br>The time zone the clock shows, for everyone on this computer.<br>`datetime.timezone` · System · Next login | Every time zone | `UTC` |
| **Set the time from the network**<br>Keep the clock right by asking time servers on the internet.<br>`datetime.network-time` · System | `true`, `false` | `false` |
| **24-hour clock**<br>14:30 rather than 2:30 PM, in the top bar and on the login and lock screens.<br>`datetime.24-hour` · System | `true`, `false` | `true` |

## Updates

| Setting | Values | Default |
|---|---|---|
| **Release channel**<br>Stable is released packages; dev is every change to main, and may break.<br>`updates.channel` · System · Next update | `stable`, `dev` | `stable` |
| **Graphics for a virtual machine**<br>In a virtual machine, draw with the host's graphics card: a second build of Mesa, from a repository of its own.<br>`updates.guest-graphics` · System · Next update | `true`, `false` | `false` |

## System

| Setting | Values | Default |
|---|---|---|
| **Computer name**<br>What this computer is called on the network, in the terminal's prompt and on the login screen.<br>`system.hostname` · System | Text, up to 64 characters | `alpymist` |
| **Password**<br>Change the password this account logs in and unlocks with, in a terminal that asks for the old one first.<br>`system.password` | An action |  |
| **Administrator**<br>Let this account change settings for everyone and install software. The last administrator cannot be turned off.<br>`system.administrator` · System · Next login | `true`, `false` | `false` |
| **Unlock with a fingerprint**<br>Let an enrolled finger unlock the screen and answer administrator prompts, beside the password. Logging in still takes the password. Fingers are added in Fingerprints.<br>`system.fingerprint` · System | `true`, `false` | `false` |
| **Hardware report**<br>Show what this computer is made of and which drivers have it, in a terminal, and offer to open it as a GitHub issue to say what does not work. It holds no serial number, address or name, and nothing is sent unless you submit the issue.<br>`system.report` | An action |  |

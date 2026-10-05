# Installing software
<!-- group: Software -->

Software comes from two places, and the Store shows both in one window:

- **Flathub**, for desktop applications. Each runs in a sandbox, and is
  installed for your account alone, so installing one needs no password.
- **Alpine's packages**, for everything else: command-line tools, libraries,
  system software. Installing one changes the system, and asks for an
  administrator's password.

## The Store

Install › Store in the menu, or search the menu for "store". Search for an
application, open its page, and install it. The same window lists what is
installed and what has updates.

```sh
alpymist-store                       # open the store
alpymist-store installed             # open at what is installed
alpymist-store updates               # open at what has updates
alpymist-store search krita          # print what a search finds
alpymist-store install flathub org.kde.krita
alpymist-store install alpine htop
alpymist-store remove flathub org.kde.krita
alpymist-store update                # update everything
```

The sources are `flathub` and `alpine`.

The Install button on [flathub.org](https://flathub.org) works too: the file
it downloads opens the Store at that application's page.

## Without the Store

The menu has the same, by name, in a terminal:

| Entry | Does |
|---|---|
| Install › Application from Flathub | Searches Flathub for what you type and installs your choice |
| Install › Alpine package | `doas apk add` the package you name |
| Remove › Flatpak application | Lists yours and removes the one you name |
| Remove › Alpine package | `doas apk del` the package you name |

And the tools themselves work as they do anywhere:

```sh
flatpak install --user flathub org.gimp.GIMP
flatpak uninstall --user org.gimp.GIMP
doas apk add htop
doas apk del htop
apk search -v editor
```

## What runs and what does not

Alpine uses musl rather than glibc as its C library, so software built only
for glibc does not run directly on it. Most such desktop software, including
proprietary applications, is on Flathub, where it brings its own libraries
and runs in a sandbox. Look there first for anything that is not an Alpine
package.

## A Flathub application built for glibc

Inside its sandbox an application has the glibc it was built for. What it
does not have is the rest of your system, and a few applications reach for
it: an editor wants your shell and your compilers, and one or two start
themselves outside the sandbox altogether. Outside there is no glibc, so
that is where they fail.

### It does not start

Run it from a terminal, to see what it says:

```sh
flatpak run dev.zed.Zed
```

"No such file or directory" about a program that is plainly there means it
was started outside the sandbox and found no glibc to run on. Whether an
application is able to do that shows in its permissions, as
`org.freedesktop.Flatpak=talk`:

```sh
flatpak info --show-permissions dev.zed.Zed
```

There is no switch that keeps every application inside: stepping out is each
application's own doing, and so is the setting that stops it, where it has
one. Its page on Flathub or its own documentation names it. It is set for
good with an override, and taken back with `--reset`:

```sh
flatpak override --user --env=ZED_FLATPAK_NO_ESCAPE=1 dev.zed.Zed
flatpak override --user --show dev.zed.Zed
flatpak override --user --reset dev.zed.Zed
```

Zed is the one the Store knows of, and it sets this as it installs Zed.
Installed with `flatpak install`, Zed needs the first line above, once.

### Its terminal is not your system

An editor that stays in its sandbox opens the sandbox's shell: no `apk`, no
`doas`, none of what you have installed. Zed and Visual Studio Code both
carry `host-spawn`, which starts your own shell outside. The Store writes
this for Zed when Zed has no settings yet; in a `settings.json` you already
have, add it yourself:

```json
{
  "terminal": {
    "shell": {
      "with_arguments": { "program": "host-spawn", "args": [] }
    }
  }
}
```

In Visual Studio Code's settings it is a terminal profile:

```json
{
  "terminal.integrated.profiles.linux": {
    "host": { "path": "/app/bin/host-spawn" }
  },
  "terminal.integrated.defaultProfile.linux": "host"
}
```

### Its language servers do not see your compilers

What an editor starts by itself, a language server or a build, still runs
inside, and the `cargo` or `go` installed from Alpine is built for musl and
does not run there. Those come from Flathub too, as extensions to the
sandbox's own system: `flatpak search org.freedesktop.Sdk.Extension` lists
them, and the editor's page on Flathub says how it is told to use one. When
that is more than the work is worth, an editor packaged for Alpine has
neither problem.

### Teaching the Store another

What the Store does for Zed is written in `store.toml`, not in the Store, so
an application of your own choosing is a few lines in your copy of it
(`alpymist-store --print-config` prints the one in use):

```toml
[[source.adjust]]
app = "org.example.Editor"
env = { EXAMPLE_STAY_IN_SANDBOX = "1" }
# Optional: a file under ~/.var/app/org.example.Editor, written when the
# application is installed and no such file is there.
file = "config/example/settings.json"
contents = '{}'
```

It goes under the `[[source]]` whose kind is `flatpak`, and is done once, as
the Store installs the application; one already installed is not touched.

## Software that is neither

A few things come from their own vendor, because nobody else may distribute
them. Alpymist does not ship them; it offers to fetch each from its vendor
when you ask, says what it is about to do, and can remove it again:

| Entry | What |
|---|---|
| Install › Jottacloud | Jottacloud's backup client, from Jottacloud |
| Install › Claude Code, Codex, Gemini CLI | The AI vendors' command-line tools; see [AI](20-ai.md) |

Each has an entry under Remove as well.

## Applications in the background

An application with a tray icon shows it behind the arrow on the bar, which
slides open under the pointer.

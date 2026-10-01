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

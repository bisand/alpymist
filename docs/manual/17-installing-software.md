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
neither problem, and nor has one in
[a box of another distribution](#a-box-of-another-distribution).

### Which editor, and where

| Editor | From Alpine | From Flathub | In a box |
|---|---|---|---|
| Neovim, Vim, Helix, Emacs, Kakoune, micro | Yes | | |
| Kate, GNOME Builder, Geany, Lapce | Yes | | |
| Visual Studio Code | | `com.visualstudio.code` | Microsoft's `.deb` |
| VSCodium | | `com.vscodium.codium` | Its `.deb` |
| Zed | | `dev.zed.Zed` | Its tarball |
| JetBrains' IDEs | | One each, under `com.jetbrains` | Its tarball, or Toolbox |
| Sublime Text | | `com.sublimehq.SublimeText` | Its apt repository |

An editor from Alpine runs on the system itself and sees everything on it:
nothing on this page applies to it, with one exception. An editor that
downloads language servers for you, as Neovim does with Mason, downloads
ones built for glibc, and they do not run. Install the server from Alpine
instead (`rust-analyzer`, `gopls`, `clang22-extra-tools` for clangd, and many
more are packages) and point the editor at it.

An editor from Flathub is right for reading and editing, and takes the
steps above before it builds anything. For work that is mostly building,
running and debugging software for glibc, put the editor in
[a box](#a-box-of-another-distribution) with its compilers.

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

## A box of another distribution

For writing software that is built for glibc, there is a third place besides
Alpine and Flathub: a whole other distribution, Debian or Fedora or Ubuntu,
running in a container with your home directory in it. Its editor is the one
its vendor builds, its compilers and language servers sit beside it, and all
of it sees your files. [Distrobox](https://distrobox.it) is the tool, on top
of podman.

Alpymist does not install any of this, and nothing in Settings or the Store
sets it up: it is a gigabyte or two and a container runtime, for the people
who need it. What follows was tried on Alpymist 0.3.7 in October 2026, with
Debian 13.

### What a box is, and is not

- **It is not a sandbox.** A box has your home directory, your Wayland
  session and your graphics card. A program in it can read and change
  everything you can. Software you would not run on the system itself does
  not become safe in a box; Flathub's sandbox is the place for that.
- **It is not root.** Podman runs as you, with no service in the background
  and nothing listening. `sudo` in a box makes you root in the box, which
  outside it is still you.
- **Its software is not Alpine's.** What you install in a box comes from
  that distribution, and the box itself from a registry on the internet.
  Neither is checked by Alpymist.

### Setting it up

Four things, once:

```sh
doas apk add podman distrobox
echo "$USER:100000:65536" | doas tee /etc/subuid /etc/subgid
doas rc-update add cgroups
doas rc-service cgroups start
```

The second gives your account a range of user ids that belong to nobody,
which is how a box has users of its own without being root; without it no
image can be unpacked. The third and fourth mount the kernel's control
groups, now and at every boot.

### Making a box and using it

```sh
distrobox create --name dev --image docker.io/library/debian:stable
distrobox enter dev
```

The first entry takes a minute while the box is made ready. After that you
are in Debian, as yourself, in your own home directory, in your own shell:
`apt` installs, and a graphical program started there opens on your desktop
and draws with your graphics card.

```sh
sudo apt install cargo                 # in the box: Debian's packages
distrobox-host-exec apk info           # in the box: a command on Alpymist
distrobox enter dev -- cargo build     # from outside: one command in the box
```

### An editor in a box

Install the vendor's own build for Linux, inside the box. Its terminal is
then the box's, and its language servers find the compilers installed there.
Its settings and extensions are kept in your home directory, apart from
those of the same editor from Flathub, which are under `~/.var/app`.

Mind where it is installed. Your home directory is shared, so anything put
under `~/.local` by a vendor's install script leaves a program in your path,
and an entry in your menu, that cannot run outside the box. A package of the
box's distribution, or a directory of the box's own such as `/opt`, has
neither problem.

**Visual Studio Code** is a package for Debian. Installing it asks whether
the box should follow Microsoft's repository for its updates:

```sh
curl -fL -o /tmp/code.deb \
  "https://code.visualstudio.com/sha/download?build=stable&os=linux-deb-x64"
sudo apt install /tmp/code.deb
distrobox-export --app code
```

`distrobox-export` puts it in your menu, started in the box; with `--delete`
it takes it out again. VSCodium is installed the same way from the `.deb` on
its releases page, and Sublime Text from its own apt repository.

**Zed** comes as a tarball and an install script. The script writes to
`~/.local`, and the menu entry it leaves takes the place of the one the
Store's Zed has, so unpack the tarball into `/opt` instead:

```sh
curl -fL -o /tmp/zed.tar.gz \
  "https://cloud.zed.dev/releases/stable/latest/download?asset=zed&arch=x86_64&os=linux"
sudo tar -xzf /tmp/zed.tar.gz -C /opt
sudo apt install libvulkan1 mesa-vulkan-drivers libxkbcommon-x11-0 libasound2t64
/opt/zed.app/bin/zed
```

**JetBrains' IDEs** are tarballs too: unpack one into `/opt` and start its
`bin/idea.sh`, or the one named for the product. They bring their own Java.

Compilers are the box's as well. Debian's are older than Alpine's: for Rust,
install `rustup` in the box and let it bring the toolchain a project asks
for.

### Taking it away

```sh
distrobox rm --force dev      # one box, and its entry in the menu
podman system reset           # every box and image
doas apk del podman distrobox
doas rc-update del cgroups
doas rm /etc/subuid /etc/subgid
```

Files a box wrote in your home directory are yours and stay.

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

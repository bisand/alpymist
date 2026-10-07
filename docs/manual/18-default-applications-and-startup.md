# Default applications and startup
<!-- group: Software -->

## Default applications

Settings › Default applications says which program opens each kind of thing.
Each list offers the installed applications that can open that kind.

| Setting | Opens | As shipped |
|---|---|---|
| Web browser | Links and web pages | LibreWolf |
| Mail | Email addresses and messages | Nothing installed |
| File manager | Folders | Thunar |
| Text editor | Text files, configuration and code | squint |
| Terminal | Commands, and programs that run in one | foot |
| Images | Pictures and photos | Ristretto |
| PDF | PDF documents | Not chosen; LibreWolf can open them |
| Video | Films and clips | mpv |
| Music | Songs and sound files | mpv |
| Archives | Zip files, tarballs | xarchiver |
| Calendar | Calendar files and subscriptions | Nothing installed |

The file manager is Thunar: <kbd>Super</kbd>+<kbd>E</kbd> opens it, as does
a keyboard's Files key, and it is among the menu's applications. An account
older than the file manager has neither that key nor the editor's until its
`hyprland.conf` is given the lines
`bind = SUPER, E, exec, alpymist open files` and
`bind = SUPER SHIFT, E, exec, alpymist open editor`, the second in place of
`bind = SUPER SHIFT, E, exit`, which logged out at once and is no longer
shipped: logging out is in the menu's System page.

Thunar shows folders as a list; Edit › Preferences in it changes that.
Windows shares and phones need `gvfs-smb` and `gvfs-mtp`, which the menu's
Install › Alpine package adds.

Pictures open in Ristretto, films and music in mpv, and archives in
xarchiver, which also gives Thunar's menu "Extract here" and "Create
archive". Ristretto opens PNG, JPEG, GIF, BMP, TIFF, SVG and WebP; a phone's
HEIF and AVIF pictures need a viewer from the Store. It shows one picture
and no strip of the folder's others, since nothing here makes thumbnails;
the arrows in its toolbar still go to the next and the last.

There is one game, Solitaire, among the menu's applications: Klondike, in
the colours of Settings › Appearance. Drag the cards, or double-click one to
send it to its foundation; click the stock to turn it. With the keyboard,
the arrows move between the piles, <kbd>Enter</kbd> picks cards up and puts
them down, <kbd>Up</kbd> and <kbd>Down</kbd> on a pile picked up from take
more cards or fewer, <kbd>Space</kbd> turns the stock, <kbd>H</kbd> sends a
card home, <kbd>U</kbd> takes a move back and <kbd>N</kbd> deals again. The
bar chooses between turning one card and three, and <kbd>B</kbd> or its
last button changes the picture on the cards' backs: twelve of them, from
mountains in mist to the home computers of the 1980s, and the one chosen is
kept.

The right-hand column is a system with nothing added, and yours shows what
you have installed. Where nothing installed opens a kind, the page says so; the Store has
applications that do, and once one is installed it appears in the list.

```sh
alpymist list default.browser            # what could be chosen
alpymist set default.browser com.vivaldi.Vivaldi
alpymist set default.terminal foot
```

<kbd>Super</kbd>+<kbd>B</kbd> and <kbd>Super</kbd>+<kbd>Return</kbd> follow
the browser and the terminal chosen here, as <kbd>Super</kbd>+<kbd>E</kbd>
and <kbd>Super</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd> do the file manager and
the text editor.

### alpymist open

`alpymist open` starts whatever is chosen for a category, which makes it the
thing to put in a keybinding or a script:

```sh
alpymist open browser https://alpinelinux.org
alpymist open editor ~/notes.md
alpymist open files ~/Downloads
alpymist open terminal -- htop
alpymist open browser --print      # show the command, start nothing
```

The categories are `browser`, `mail`, `files`, `editor`, `terminal`,
`images`, `pdf`, `video`, `music`, `archives` and `calendar`.

## Programs that start at login

Settings › Startup lists the programs that start when you log in: those a
package installed to start, each with a switch, and those you add.

- **Add a program** offers every installed application. Choose one and it
  starts at every login.
- Switch one off and it no longer starts. Nothing is uninstalled.

```sh
alpymist list startup
alpymist autostart --dry-run       # what would be started
```

Changes apply at the next login. For a command that is not an installed
application, add a line to your own `~/.config/hypr/hyprland.conf`:

```
exec-once = syncthing --no-browser
```

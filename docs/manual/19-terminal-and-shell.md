# Terminal and shell
<!-- group: Software -->

## The terminal

<kbd>Super</kbd>+<kbd>Return</kbd> opens `foot`, a small, fast terminal.

| Key | Does |
|---|---|
| <kbd>Super</kbd>+<kbd>C</kbd> / <kbd>V</kbd> | Copy and paste, as everywhere |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> / <kbd>V</kbd> | The terminal's own copy and paste |
| <kbd>Shift</kbd>+<kbd>PgUp</kbd> / <kbd>PgDn</kbd> | Scroll back |
| <kbd>Ctrl</kbd>+<kbd>+</kbd> / <kbd>-</kbd> | Larger and smaller text |

Its font and behaviour are in `~/.config/foot/foot.ini` (Setup › Config ›
Terminal); its colours follow Settings › Appearance. Another terminal
installed from the Store can be made the default in Settings › Default
applications.

## The shell

The shell is `zsh`, with:

- **Suggestions** from your history as you type, in grey:
  <kbd>→</kbd> accepts one.
- **Syntax highlighting**: a command that does not exist is red before you
  press <kbd>Enter</kbd>.
- **Completion** on <kbd>Tab</kbd> for commands, options and files. After
  `alpymist` it knows the settings and what each takes:
  `alpymist set touchpad.` and <kbd>Tab</kbd> lists the touchpad's, and
  <kbd>Tab</kbd> after one of them lists its values. `bash` and `fish`,
  installed from the Store, are given the same.
- **oh-my-zsh's git plugin**, which gives short names such as `gst` for
  `git status` and `gco` for `git checkout`.
- **The starship prompt**, showing who and where you are, the directory, the
  git branch and its state, the language a project is written in, and the
  time.

Your `~/.zshrc` is yours (Setup › Config › Shell), and the prompt is in
`~/.config/starship.toml` (Setup › Config › Prompt). All of it is installed
and updated as packages; oh-my-zsh's own updater is turned off.

## What is installed

| For | Tools |
|---|---|
| Version control | `git`, `gh` |
| Networks | `ssh`, `curl`, `rsync` |
| Editing | `nano` in a terminal, `squint` on the desktop |
| Reading | `less`, `man` |
| Files | `unzip` |

`man git` works: the manuals of these tools are installed with them.

There are also a few programs that fill a terminal's window:

| For | Program |
|---|---|
| A git repository | `lazygit` |
| Music and internet radio | `cliamp`; <kbd>R</kbd> lists the stations |
| What the machine is doing | `btop` |
| What fills the disk | `ncdu` |
| Several terminals in one, that outlive it | `tmux` |
| What the machine is | `fastfetch` |

They come with the desktop, as the package `alpymist-tui`. cliamp is in
the menu too. Its `cliamp upgrade` is not how it is updated here: it comes with the
system's updates, like everything else.

## Editors

`squint` is the desktop's text editor: it is what the menu's Config entries
open, and what `VISUAL` names for programs that ask for an editor. It opens files of any size at once. In a terminal outside the
desktop, such as over SSH, the editor is `nano`.

To use another editor everywhere, install it and set `VISUAL` in your
`~/.config/hypr/hyprland.conf`, in place of the line that names squint:

```
env = VISUAL,code --wait
```

## Running something as administrator

```sh
doas apk add btop
```

`doas` is Alpine's `sudo`. It asks for your own password.

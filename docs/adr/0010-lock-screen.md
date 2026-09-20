# ADR 0010 — The lock screen is the login screen

**Status:** accepted · **Date:** 2026-09-20

## Context

Locking was `swaylock -f -c 0b121e`: a blue screen with a circle in the middle
of it. Six things ran it — `Super+L` under Hyprland, `W-l` and the menu under
labwc, the system menu's *Lock*, a lid set to lock, and the idle watch at the
blank timeout and before sleeping — so it was the one part of Alpymist that
everybody saw regularly and that looked like nothing else in it. It does not
say whose session it is, what time it is, or that it is Alpymist at all.

[ADR 0009](0009-screensaver-and-idle.md) left this open on purpose:

> If a lock screen of Alpymist's own is ever wanted, the drawing here is what
> it would show, and `ext-session-lock-v1` is where it would go. Nothing here
> forecloses that; it is a separate decision, and this one does not pretend to
> have made it.

This is that decision.

A lock screen has to do four things: cover every output, hold the keyboard,
stay up even if the program drawing it dies, and check a password. Only the
last one is ours to invent; the other three are `ext-session-lock-v1` and the
compositor.

## Decision

### 1. It is an `ext-session-lock-v1` client, not a layer surface

ADR 0009 wrote down what an overlay surface costs: "an overlay surface is not a
guard. A compositor crash, or the process being killed, uncovers the screen."
That is exactly why the screensaver is one and the lock is not. With a session
lock the compositor blanks every output the moment the lock is *asked for*,
shows nothing but this client's surfaces until it is told the session is
unlocked, and keeps the session covered if this process dies. `kill` does not
uncover the screen; it strands it.

It also answers ADR 0003's lookalike question by construction. Anything can
draw a convincing fake lock screen on a layer surface — but it cannot be *the*
lock, because the compositor grants one session lock at a time and while a real
one is up there is nothing else on screen at all. Ctrl+Alt+Delete still answers
the other direction, as it does for the polkit prompt.

The cost of the protocol is the failure mode it is built on: a lock that cannot
check a password is a session that has to be rescued from a text console
(Ctrl+Alt+F2, `loginctl unlock-session` or killing the compositor). That is why
§5 is about failing *before* the screen is covered.

### 2. It is the greeter's screen, not a copy of it

`alpymist-greeter` was already a library whose `App` knows nothing about
greetd: it takes a list of accounts, an `Authenticator` closure, a size, input
as actions and a canvas to draw on. `alpymist-lock` builds that same `App` with
PAM in place of greetd. The mountains, the clock, the card, the caret, the
message line and the wrapping are one implementation, drawn from one place.

Three things differ, and they are a `Purpose` on the `App` rather than a second
screen:

- **One account**, this session's, found by user id in `/proc/self/status`
  rather than by `$USER` — the environment is whatever started the session and
  can say anything. There is nobody to switch to: switching users is what the
  login screen is for, and it is behind Log out.
- **No footer buttons.** *F11 Restart* and *F12 Power off* belong on a login
  screen and not on a locked one. The whole of that is `purpose ==
  Purpose::Login` in three places.
- **"Enter  Unlock"** on the button instead of "Enter  Log in".

Caps Lock is said in the message line, which the login screen has no way to
know about — it reads evdev, where the lock is told by the compositor.

### 3. PAM directly, as its own service, and only `pam_authenticate`

`/etc/pam.d/alpymist-lock` is `auth include base-auth`: the same stack
`/etc/pam.d/alpymist-greetd` includes, so what unlocks the screen is what logs
the account in. An account with no password unlocks on Enter for the same
reason it logs in on Enter, and a machine with such an account is not made safe
by a lock screen.

It runs as the account, not as root: `pam_unix` hands the check to its own
setuid `unix_chkpwd`, so nothing here ever reads `/etc/shadow`.

**Account management is deliberately not asked.** `pam_acct_mgmt` answers
"may this account log in" — expiry, allowed hours — and a lock screen is not a
login. Refusing somebody entry to a session that is already running, over a
password that expired while they were at lunch, answers a question nobody
asked and loses their work.

Two alternatives were considered:

- **polkit**, reusing `alpymist-auth`'s conversation with
  `polkit-agent-helper-1`, which already runs the whole PAM exchange. It would
  have added no new dependency at all. It was declined because it puts polkitd
  between somebody and their own session: one more daemon that has to be alive
  for a locked screen to open.
- **Writing the PAM conversation ourselves** over raw bindings. The callback
  PAM wants is an `extern "C"` function that allocates the replies, and ADR
  0004 keeps `unsafe` to one crate that does one job. `nonstick` is a safe
  wrapper whose bindings are written out rather than generated, so the builder
  needs `linux-pam-dev` and not clang. Linking a C library through a crate that
  contains the `unsafe` is what `alpymist-menu` already does for
  libxkbcommon; ADR 0004's rule — no `unsafe` in first-party code — is intact,
  and `alpymist-lock` carries `#![forbid(unsafe_code)]` like every other crate.

### 4. `-f` runs the program again rather than forking

swayidle runs one command before sleeping and waits for it, so the command has
to return when the screen is covered and not before, or the machine sleeps with
the desktop still showing. `swaylock -f` forks. Forking is `unsafe` in Rust, so
`alpymist-lock -f` starts `alpymist-lock` instead, waits to be told down a pipe
that the first frame is committed, and returns. The second run is what stays.

The spelling is swaylock's because the setting that holds this command is the
person's to change (`~/.config/alpymist/power.toml`), and `alpymist-lock -f`
being the same shape as what was there is one less thing to explain.

### 5. Everything that can fail, fails before the screen is covered

Locking a session is easy to do and hard to undo. So `alpymist-lock` checks,
in this order and before it asks for the lock: that there is a Wayland session,
that `/etc/pam.d/alpymist-lock` exists, that this session's account can be
found, and that the compositor has `ext-session-lock-v1`. Each of those is a
message on the terminal and an exit code, with the desktop untouched.

After the lock is granted there is nothing left that can fail but drawing, and
a lock that draws nothing is still a lock.

### 6. The compositor draws the pointer, and the callback is only a hint

Two things were measured on a VM with no GPU acceleration — virtio-gpu, so
Hyprland composites in software — and both are worth writing down, because
neither is visible on a machine where the GPU hides it.

**Following the pointer costs a screen.** The login screen draws its own
pointer, because on DRM there is nothing else to draw one. Doing the same here
meant a full frame for every motion event, and a compositor that re-renders a
screen behind every one of them: moving the mouse made the whole machine
crawl. So `App` draws a pointer only for `Purpose::Login`, the compositor's own
is left alone, and the lock asks about the pointer only where it was clicked.

**Damage is a courtesy, not a contract.** The surface is opaque — `Xrgb8888`
plus an opaque region, so a compositor has nothing to blend — and it reports
only what changed, which is the card, the clock's band and where the pointer
was. Measured over 100 keystrokes, Hyprland spent the same on all three of
damaging the card, damaging the whole screen, and a deliberate one-pixel lie:
272, 276 and 270 hundredths of a second. It re-renders the monitor whichever
it is told. The reporting stays because it is correct and costs nothing, and
because the next compositor may believe it.

What the same hundred keystrokes cost *this* program: 12 hundredths, about
half a millisecond a frame, with frames going out every 10 ms.

**A compositor may never ask for a frame at all.** A client paints when its
frame callback arrives, and a compositor with nothing else to repaint — no
windows, no animation, nobody moving the mouse, which is exactly a locked
screen — may not send one. The symptom was a password that appeared only when
the mouse was jogged. So the callback is a hint: if none has arrived in 80 ms
and there is something to show, it is shown anyway. Below noticing, far above
any frame time here, and never reached by a compositor behaving normally.

## Consequences

- `swaylock` comes off `alpymist-desktop-wayland` and `alpymist-power`, and
  every one of the six call sites runs `alpymist-lock`. `power.toml` still
  accepts any command, so anybody who preferred swaylock can put it back in one
  line — and the test that proves it round-trips uses swaylock, on purpose.
- `alpymist-lock` is a package, and `alpymist-greeter` is now a library two
  programs are built from. Building the lock builds the greeter's screen.
- The Legacy tier gains nothing, as with the screensaver: X11 has `i3lock` and
  none of this.
- **Two outputs of different sizes recompose the scene on every frame**, because
  the `App` is one screen's worth of state laid out for one size and is drawn on
  each output in turn. Equal-sized outputs, and the single screens these
  machines have, cost nothing. Making the picture per-output would mean a
  scene, a layout and a password field per output, and the second password
  field is the part that is actually wrong.
- The greeter's `App::draw` now returns the region that may differ from the
  frame before, which the login screen is free to ignore and does.
- Nothing here shows the screensaver behind the lock, and nothing here unlocks
  by fingerprint. Both would be this program's to grow; neither is missed yet.

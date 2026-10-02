# ADR 0017 — AI usage in the bar: providers as programs, keys in the keyring

**Status:** accepted · **Date:** 2026-10-01

## Context

People who work with AI models pay for them in two ways that are both easy to
run out of without noticing: a subscription with limits over a window of time,
and an API key that spends money. The vendors show each on a web page of their
own. #16 asks for one place: an icon in the bar beside Wi-Fi and the battery.

What can actually be read differs by vendor, and is written down in
[`docs/ai-usage-providers.md`](../ai-usage-providers.md). In short: some have
a documented API for an ordinary key, some only for an administrator's, some
tell only their own command-line tool, and a few say nothing at all. The list
will not stay as it is for a year.

## Decision

**A provider is a program and a file, not a branch in a match**, as a
screensaver is (ADR 0009). `/usr/share/alpymist/ai-usage/<id>.toml` gives its
name, the keys it needs, how often it may be asked, and the program to run.
Nothing in `alpymist-ai-usage` names any provider. Adding one is a package
with those two things in it, from anyone.

**The program's whole protocol is one JSON document each way.** It is handed
`{"credentials": {…}}` on its standard input, and prints a report: a plan
name, and meters that are each a limit window (a fraction used and a reset
time), a spend, or a balance. On failure it says why on its standard error.
Keys never travel as arguments or in the environment, where any process on
the machine could read them. The same holds one step further: the shipped
providers ask their vendors through `curl --config -`, so a key in a header
is never an argument either.

**Keys live in the keyring**, through `secret-tool`, and nowhere else
(ADR 0013). The settings file names which providers are on and has no key in
it. A locked keyring means no asking, and the bar says so. The keyring is
asked where it is first, once, without making it ask anyone anything: touching
a locked keyring puts up a dialog to unlock it, and the bar would do that every
few minutes with nobody there. A keyring that does not answer at all, as after
a login that had no password, is said the same way, and every call to it has a
time limit.

**Nothing is on by default.** With no provider turned on, the bar's module
prints nothing and is hidden. Turning one on is `alpymist-ai-usage enable`.

**A vendor's own tool is installed the vendor's way, when its provider is
turned on, and never shipped by Alpymist.** A provider's file may name a
program it reads through and the vendor's installer for it. Turning the
provider on shows that command and asks before running it, in the terminal
where it can be watched. Claude Code is the first: Anthropic's installer puts
it in the account's own `~/.local/bin`.

**Documented routes ship unmarked. Undocumented ones ship marked.** A reader
that uses an endpoint its vendor does not document says `unofficial = true`
in its file, and turning it on says that it may stop working. One route is
not taken at all: Claude Code's saved login would give a subscription's
limits at any time, and Anthropic's terms keep that login for Claude Code.
The Claude provider uses the documented route instead. Claude Code hands its
status line command the limits; Alpymist's command keeps them and then runs
whatever status line was there before, so nothing on screen changes.

**The last answer is kept and shown with its age.** Offline, or with a
provider that cannot be reached, the bar shows what was last known, never a
blank. The time of the last try, answered or not, paces the next.

**It asks rarely.** No more often than the provider's file allows, nor than
the settings ask; not while the screen is locked; and not on a battery below
a threshold with nothing charging it.

## Consequences

- The bar's icon shows whichever provider is closest to a limit. A provider
  that reports only a spend, with no budget, has no limit to be close to: it
  is in the tooltip and never drives the icon. That is every admin-key API
  today, because none says what is left.
- The Claude figures are only as fresh as the last use of Claude Code on this
  account. A window whose reset time has passed is shown as unused, which is
  true unless Claude was used elsewhere since.
- Turning the Claude provider on edits `~/.claude/settings.json`, which is
  Claude Code's. Only `statusLine` is touched, the one that was there is kept
  and put back when the provider is turned off, and the file's keys come out
  in alphabetical order, which Claude Code does not mind.
- `curl` and `secret-tool` are run as programs. That is two fewer libraries
  with TLS and D-Bus in them to build and keep current, at the cost of a
  process per asking, a few times an hour.
- Thresholds with notifications, a popup under the bar, and a page in
  Settings are not here yet. The settings file has the fields they will use.

## Addendum — 2026-10-01: a page in Settings, a notification, and one asker

The last consequence above listed what was not there yet. Two of the three
now are.

**Settings › AI usage** has a switch for each provider installed, read from
the same files, and when to warn, whether to notify, and how often to ask. A
provider with everything it needs is turned on by its switch. One that needs a
key, or its vendor's tool installed, opens a terminal on
`alpymist-ai-usage enable`, which asks: a key is never a setting's value,
because that would be an argument any process could read, and an installer
should be seen running.

**A notification** is sent when a provider passes the warning, and one more
when it is nearly used up. What was last said is kept with the provider's
report, so it is said once for each crossing, and again only after the
provider has been back under.

**Only one process asks a provider at a time.** The bar runs once for every
screen, and each of them finds the same provider due at the same moment. A
lock file beside the kept report decides which one asks and notifies; the
others show what it kept.

**A popup under the bar** opens at a click on the icon, as Wi-Fi's and the
battery's do: every provider turned on, each meter with what it says and a
bar where it has a limit, and how old the answer is where that matters. It
reads what is kept and asks nobody by being opened; a button asks them all
again, and another opens Settings. `alpymist-ai-usage` with no command is
the popup, as `alpymist-wifi` and `alpymist-power` are, and what it used to
print is `alpymist-ai-usage status`.

## Addendum — 2026-10-01: the undocumented readers

GitHub Copilot, ChatGPT's Codex limits and Gemini CLI's quota ship, marked
`unofficial = true`, as the decision above allowed. What was settled in
writing them:

- **They are handed no key and keep none.** Each uses the login its vendor's
  own tool saved. Copilot's never leaves `gh`: the reader runs `gh api`. The
  other two read a token from the tool's file and send it to that vendor's
  own host, in curl's configuration on standard input, as every key here is.
- **A login is read, never written and never renewed.** Renewing Gemini
  CLI's takes Google's client secret for Gemini CLI, and renewing Codex's
  would mean writing Codex's file under it. So Gemini's figures are as fresh
  as Gemini CLI's last use, within the hour its login lasts, and Codex's
  stop when its login runs out until Codex is used again. Both say so.
- **Claude Code's login is still not read.** Anthropic's terms say third
  parties may not use it; nothing found in the other three vendors' terms
  says the same of theirs, and that was not a lawyer's reading.

`docs/ai-usage-providers.md` has the fields, and says which of the three
have been run against a live answer.

## Addendum — 2026-10-01: the popup is where a provider is set up

The popup shipped earlier the same day showed figures and two buttons, and
setting a provider up meant a terminal that nothing pointed to. That was
not what was wanted: the Wi-Fi popup is the model, where joining a network
is where its passphrase is asked.

- **Every provider installed has a row and a switch**, on or off. Turning
  one on is where it is set up.
- **A key is typed in the popup**, in a field under the provider's row, and
  goes from there to the keyring. It is never an argument and never in a
  file, as before. A provider that is on has a button to give its keys
  again.
- **A vendor's tool is still installed in a terminal**, from a button under
  the row. An installer is somebody else's script, and should be seen
  running; the earlier decision stands.
- **Popups take a paste.** Ctrl+V and Shift+Insert hand a widget the
  clipboard's first line as if typed, through `wl-paste`. This is in
  `alpymist-widget`, so the Wi-Fi popup's passphrase field has it too. A
  pasted key is in the clipboard history like anything else copied.
- **Notify and the warning** are in the popup as well as in Settings.
- **Settings opens the popup** for a provider that needs something
  (`alpymist-ai-usage setup ID`), where it used to open a terminal.

The bar's icon is still hidden while no provider is on, so the first one is
turned on from Settings, the menu, or `alpymist-ai-usage`.

## Addendum — 2026-10-01: the icon is always in the bar

The decision said that with no provider turned on the bar's module prints
nothing and is hidden, and the addendum above left it so. That made the
popup, which is now where a provider is turned on, unreachable from the bar
until one already was. The icon is now always shown, dimmed while nothing is
on, with a tooltip saying a click turns one on. What that gave up: everyone
with the desktop installed has an icon in the bar for something they may
never use. Removing `custom/ai` from the bar's modules takes it away.

## Addendum — 2026-10-01: the vendors' tools in the menu

The tools the providers are read through are worth having for their own
sake, so the menu has them: an AI menu that starts Claude Code, Codex and
Gemini CLI in a terminal, and each under Install and Remove, as Jottacloud
is.

- **A provider's file says how its tool is removed**, beside how it is
  installed: `remove`, a shell line, shown and asked about before it runs.
  It takes away what the installer put down and nothing else; a tool's
  login and settings stay.
- **`alpymist-ai-usage tool run | install | remove ID`** does it. Starting
  one that is not installed offers to install it first; installing again
  is how each updates; removing one turns its provider off, which could no
  longer be read.
- **Still never a dependency.** The menu's entries are in a file of the
  package's, `/usr/share/alpymist/menu.d/ai-usage.toml`, and
  `tests/definitions.rs` holds each to a shipped provider that says how its
  tool is removed.
- A tool is started with `~/.local/bin` on its `PATH`, where two of the
  three installers put it and where the desktop's own `PATH` does not look.

## Addendum — 2026-10-01: a provider that reads this machine is asked every minute

Turning Claude on and then using Claude Code left the bar saying "Claude Code
has not said yet" for ten minutes after it had. The limits were in the file
the status line keeps within seconds; the bar did not read it, because a
provider is asked no more often than Settings' *refresh every*, ten minutes by
default, and the first asking, at the moment it was turned on, had found
nothing.

That interval is there to spare a vendor's service and a laptop's radio.
Reading a file spares neither. A provider's file may now say `local = true`:
asking it reaches no one, and it is asked as often as its own `refresh`
allows, whatever Settings says. Claude's does, with `refresh = 60`. No other
shipped provider does, and `tests/definitions.rs` holds that: one that calls
its vendor and says `local` would be asked every minute.

The locked screen and the low battery still stop it, as they stop everything.

## Addendum — 2026-10-02: and as often as the bar looks

A minute was still slow. Claude Code's status line has the limits in its file
within seconds of an answer, and the bar showed them up to eighty seconds
later: a minute between askings, and twenty seconds between the bar's looks
at whether one was due.

A provider that says `local` may now be asked every ten seconds, and Claude's
file says `refresh = 10`; the bar looks every ten seconds too. One that calls
its vendor is still held to once a minute at the very least, and to Settings'
*refresh every* above that. What this costs is a small program run and a file
read every ten seconds while the screen is unlocked.

## Addendum — 2026-10-02: a status line for the account that has none

The decision above says Alpymist's status line command "runs whatever status
line was there before, so nothing on screen changes". For an account with no
status line that meant Claude Code showed none, and the limits it was handed
went to the bar and nowhere the person using it was looking.

**Alpymist ships a status line, and an account without one has it.**
`/usr/share/alpymist/claude-statusline.sh`, from this package: the account,
the directory, git, the model, how full the context is, the 5-hour and
weekly limits and the time, in the shape and the colours of the starship
prompt beside it. It is bash and `jq`, which the package now depends on, and
it asks nobody anything.

- **A new account starts with it.** `/etc/skel` has a
  `~/.claude/settings.json` that names it and holds nothing else, so it is
  there before Claude Code is, however Claude Code is then installed.
- **Turning the Claude provider on gives it to an account that had none**,
  as the status line that was there: run after Alpymist's command, and left
  in place when the provider is turned off. An account that turned the
  provider on before this, and so has nothing kept, gets it too.
- **A status line of the account's own is never replaced.** One already in
  Claude Code's settings is kept and run, as before.

What this changes of the above: something on screen does change now, for an
account that had no status line. An account that wants none while the
provider is on has no switch for that; it names a status line of its own
that prints nothing. An existing account with the provider off and Claude
Code installed is not touched, and has it by turning the provider on or by
naming the script in its settings. And a new account has a `~/.claude`
directory whether or not Claude Code is ever installed: one small file.

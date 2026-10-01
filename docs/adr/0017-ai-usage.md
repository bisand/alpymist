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

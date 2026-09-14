# XDG_RUNTIME_DIR for logins that do not go through PAM.
#
# A login through the login screen gets /run/user/<uid> from pam_rundir. A
# login at a text console does not: Alpine's getty runs BusyBox login, which
# has no PAM at all, and every Wayland compositor refuses to start without
# this set. So make one here, the way the Alpine wiki suggests, and only when
# nothing else has.
#
# In /tmp, which anyone can write to, so the directory is used only if it is
# ours and nobody else can read it: otherwise someone could create it first and
# watch the compositor's sockets.
if [ -z "$XDG_RUNTIME_DIR" ] && [ "$(id -u)" != 0 ]; then
	_alpymist_rundir="/tmp/$(id -u)-runtime-dir"
	if [ ! -e "$_alpymist_rundir" ]; then
		mkdir -m 0700 "$_alpymist_rundir" 2>/dev/null
	fi
	if [ -d "$_alpymist_rundir" ] && [ ! -L "$_alpymist_rundir" ] \
		&& [ "$(stat -c '%u %a' "$_alpymist_rundir" 2>/dev/null)" = "$(id -u) 700" ]; then
		export XDG_RUNTIME_DIR="$_alpymist_rundir"
	fi
	unset _alpymist_rundir
fi

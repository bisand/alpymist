# Why the desktop stopped, for whoever logs in to find out.
#
# When a desktop stops as soon as it starts, the login screen says so and
# offers Ctrl+Alt+F2, a text console; logging in there, or over SSH, lands
# here. The login screen writes /var/cache/alpymist-greeter/stopped as
# "USER BOOT_ID", and it is about this login only for this account in this
# boot. `alpymist session` keeps what the desktop printed in session.log, and
# the end of that is usually the reason. It says so until the next login at
# the login screen, which clears the file.
_alpymist_stopped=/var/cache/alpymist-greeter/stopped
case $- in
*i*)
	if [ -r "$_alpymist_stopped" ] &&
		[ "$(cat "$_alpymist_stopped")" = "$(id -un) $(cat /proc/sys/kernel/random/boot_id)" ]; then
		_alpymist_log="${XDG_STATE_HOME:-$HOME/.local/state}/alpymist/session.log"
		printf '\nYour desktop stopped as soon as it started.\n'
		if [ -s "$_alpymist_log" ]; then
			printf 'The end of what it said, from %s:\n\n' "$_alpymist_log"
			tail -n 20 "$_alpymist_log"
		else
			printf 'It said nothing: %s is empty or missing.\n' "$_alpymist_log"
		fi
		printf '\nCtrl+Alt+F7 goes back to the login screen.\n\n'
	fi
	;;
esac
unset _alpymist_stopped _alpymist_log

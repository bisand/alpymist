# Alpymist's zsh: oh-my-zsh and plugins from Alpine's packages, starship for
# the prompt. Everything here is installed and updated by apk, signed like the
# rest of the system.

# A terminal can name itself something this machine has no terminfo entry for
# -- Ghostty's xterm-ghostty over SSH, say, which Alpine ships only as
# `ghostty`. zsh then cannot move the cursor, and every redraw the plugins
# below make leaves the line garbled. Fall back to the name without its xterm-
# prefix, then to plain xterm-256color, which every modern terminal speaks.
zmodload zsh/terminfo
if [[ -n $TERM && -z $terminfo[cuu1] ]]; then
	for t in "${TERM#xterm-}" xterm-256color; do
		if [[ -e /usr/share/terminfo/${t[1]}/$t || -e /etc/terminfo/${t[1]}/$t ]]; then
			export TERM=$t
			break
		fi
	done
fi

export ZSH=/usr/share/oh-my-zsh

# oh-my-zsh's own updater runs `git pull` in its directory, which would fetch
# unsigned code outside apk; here it cannot even write there. apk updates it.
zstyle ':omz:update' mode disabled

# No oh-my-zsh theme: starship draws the prompt.
ZSH_THEME=""
plugins=(git)
source "$ZSH/oh-my-zsh.sh"

source /usr/share/zsh/plugins/zsh-autosuggestions/zsh-autosuggestions.zsh
source /usr/share/zsh/plugins/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh

# The prompt is drawn with Nerd Font glyphs, which the Linux console has no
# font for; there, a plain prompt instead of a row of boxes.
if [[ $TERM == linux ]]; then
	PROMPT='%n@%m %~ %# '
else
	eval "$(starship init zsh)"
fi

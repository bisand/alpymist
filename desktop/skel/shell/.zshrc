# Alpymist's zsh: oh-my-zsh and plugins from Alpine's packages, starship for
# the prompt. Everything here is installed and updated by apk, signed like the
# rest of the system.

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

eval "$(starship init zsh)"

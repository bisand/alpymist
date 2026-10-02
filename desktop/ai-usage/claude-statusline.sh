#!/bin/bash
# Alpymist: Claude Code's status line, in the colours and the shape of the
# starship prompt beside it. From alpymist-ai-usage, as
# /usr/share/alpymist/claude-statusline.sh.
#
# It is the status line of an account that has none of its own: /etc/skel's
# ~/.claude/settings.json names it, and turning the Claude provider on gives
# it to an account without one. To change it, copy it, change the copy, and
# name the copy in ~/.claude/settings.json:
#
#   "statusLine": { "type": "command", "command": "~/.claude/statusline.sh" }
#
# Claude Code hands it a JSON document on standard input and shows what it
# prints. It asks nobody anything: jq reads the document, git the directory.
# Glyphs are FiraCode Nerd Font's, as the prompt's are.

input=$(cat)

# --- Data from Claude Code ----------------------------------------------------
model_name=$(printf '%s' "$input" | jq -r '.model.display_name // "Claude"')
model="${model_name%% (*}"   # drop trailing parenthetical, e.g. "Opus 4.8 (1M context)"
cwd=$(printf '%s' "$input" | jq -r '.workspace.current_dir // .cwd // empty')
[ -z "$cwd" ] && cwd="$PWD"
cost_usd=$(printf '%s' "$input" | jq -r '.cost.total_cost_usd // empty')
lines_added=$(printf '%s' "$input" | jq -r '.cost.total_lines_added // 0')
lines_removed=$(printf '%s' "$input" | jq -r '.cost.total_lines_removed // 0')
output_style=$(printf '%s' "$input" | jq -r '.output_style.name // empty')
model_id=$(printf '%s' "$input" | jq -r '.model.id // empty')
transcript=$(printf '%s' "$input" | jq -r '.transcript_path // empty')

# --- Context window usage -----------------------------------------------------
# Claude Code already supplies a `context_window` object with the TRUE window
# size for this session (200k vs 1M) and the current input-side usage. Use it
# directly — don't guess the limit from the model name (the "(1M context)"
# marker isn't always present) and don't re-parse the transcript.
ctx_pct=""
used=$(printf '%s' "$input" | jq -r '.context_window.total_input_tokens // empty')
limit=$(printf '%s' "$input" | jq -r '.context_window.context_window_size // empty')

# Fallback for older CLIs that don't emit context_window: sum the most recent
# assistant turn from the transcript and infer the window.
if [ -z "$used" ] || [ -z "$limit" ] || [ "$limit" -le 0 ] 2>/dev/null; then
  if [ -n "$transcript" ] && [ -f "$transcript" ]; then
    last_usage=$(tac "$transcript" 2>/dev/null | grep -m1 '"input_tokens"')
    used=$(printf '%s' "$last_usage" | jq -r '((.message.usage.input_tokens//0)+(.message.usage.cache_creation_input_tokens//0)+(.message.usage.cache_read_input_tokens//0))' 2>/dev/null)
  fi
  # 1M variant flagged in the display name, or any usage that couldn't fit a
  # 200k window must be a 1M window.
  case "$model_id $model_name" in *1m*|*1M*) limit=1000000 ;; *) limit=200000 ;; esac
  [ -n "$used" ] && [ "$used" -gt 200000 ] 2>/dev/null && limit=1000000
fi

if [ -n "$used" ] && [ "$used" -gt 0 ] 2>/dev/null && [ "${limit:-0}" -gt 0 ] 2>/dev/null; then
  ctx_pct=$(( used * 100 / limit ))
  # compact "used" for display: 29175 -> 29k
  if [ "$used" -ge 1000 ]; then used_disp="$(( used / 1000 ))k"; else used_disp="$used"; fi
fi

# --- Alpymist's palette (truecolor) -------------------------------------------
# palettes.alpymist in the prompt's starship.toml: from the wallpaper, lit
# peaks down to the night sky, text in the snow's ink.
INK="234;240;246"      # color_ink    #eaf0f6  (segment text)
PEAK="59;110;141"      # color_peak   #3b6e8d  (account)
SLOPE="50;93;122"      # color_slope  #325d7a  (directory)
RIDGE="42;76;103"      # color_ridge  #2a4c67  (git)
VALLEY="34;60;84"      # color_valley #223c54  (languages, model)
DUSK="27;45;65"        # color_dusk   #1b2d41  (context, limits)
NIGHT="20;32;48"       # color_night  #142030  (time; text on the light ones)
MIST="175;194;214"     # color_mist   #afc2d6  (limits half used)
WARN="217;160;127"     # color_warn   #d9a07f  (three quarters)
BAD="217;143;127"      # color_bad    #d98f7f  (nearly all)

SEP=$(printf '\356\202\260')    # U+E0B0 pointy right triangle (between segments, as the prompt has)
CAP_L=$(printf '\356\202\266')  # U+E0B6 left rounded half-circle (opening cap)
CAP_R=$(printf '\356\202\264')  # U+E0B4 right rounded half-circle (closing cap)
RESET=$'\e[0m'

fg() { printf '\e[38;2;%sm' "$1"; }
bg() { printf '\e[48;2;%sm' "$1"; }

# segment <bg> <fg> <text>; tracks previous bg to draw separators
prev_bg=""
out=""
segment() {
  local sbg="$1" sfg="$2" text="$3"
  if [ -z "$prev_bg" ]; then
    out+="$(fg "$sbg")$CAP_L$RESET"          # opening rounded cap
  else
    out+="$(fg "$prev_bg")$(bg "$sbg")$SEP$RESET"
  fi
  out+="$(bg "$sbg")$(fg "$sfg") ${text} $RESET"
  prev_bg="$sbg"
}

# --- Account (peak) ----------------------------------------------------------
# Alpine's mark, as the prompt shows on Alpymist.
segment "$PEAK" "$INK" "  ${USER}"

# --- Directory (slope) --------------------------------------------------------
dir="$cwd"
case "$dir" in
  "$HOME") dir="~" ;;
  "$HOME"/*) dir="~/${dir#"$HOME"/}" ;;
esac
# the last three components, as the prompt shows it
IFS='/' read -ra parts <<< "$dir"
n=${#parts[@]}
if [ "$n" -gt 3 ]; then
  dir="…/${parts[n-3]}/${parts[n-2]}/${parts[n-1]}"
fi
# directory icon substitutions
dir="${dir//Documents/󰈙 }"
dir="${dir//Downloads/ }"
dir="${dir//Pictures/ }"
dir="${dir//Developer/󰲋 }"
segment "$SLOPE" "$INK" " $dir"

# --- Git branch + status (ridge) ----------------------------------------------
if git -C "$cwd" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  branch=$(git -C "$cwd" branch --show-current 2>/dev/null)
  [ -z "$branch" ] && branch=$(git -C "$cwd" rev-parse --short HEAD 2>/dev/null)
  status=""
  git -C "$cwd" diff --quiet 2>/dev/null || status+="!"
  git -C "$cwd" diff --cached --quiet 2>/dev/null || status+="+"
  [ -n "$(git -C "$cwd" ls-files --others --exclude-standard 2>/dev/null)" ] && status+="?"
  ahead=$(git -C "$cwd" rev-list --count '@{u}..HEAD' 2>/dev/null)
  behind=$(git -C "$cwd" rev-list --count 'HEAD..@{u}' 2>/dev/null)
  [ "${ahead:-0}" -gt 0 ] 2>/dev/null && status+="⇡${ahead}"
  [ "${behind:-0}" -gt 0 ] 2>/dev/null && status+="⇣${behind}"
  gtext=" $branch"
  [ -n "$status" ] && gtext+=" $status"
  segment "$RIDGE" "$INK" "$gtext"
fi

# --- Node version (valley) ----------------------------------------------------
if [ -f "$cwd/package.json" ] || ls "$cwd"/*.{js,mjs,ts,jsx,tsx} >/dev/null 2>&1; then
  nodev=$(node --version 2>/dev/null)
  [ -n "$nodev" ] && segment "$VALLEY" "$INK" " $nodev"
fi

# --- Model (valley) -----------------------------------------------------------
segment "$VALLEY" "$INK" "󰚩 $model"

# --- Context window usage (dusk) ----------------------------------------------
if [ -n "$ctx_pct" ]; then
  segment "$DUSK" "$INK" "󰍛 ${used_disp} ${ctx_pct}%"
fi

# --- Output style (dusk) — only when not the default --------------------------
if [ -n "$output_style" ] && [ "$output_style" != "default" ]; then
  segment "$DUSK" "$INK" "󰏘 $output_style"
fi

# --- Plan usage (5-hour + weekly limits) — colour shifts with pressure -------
# Claude Code supplies live plan-limit consumption via `rate_limits` in the
# stdin JSON. These fields are only present for Claude.ai Pro/Max sessions and
# only after the first API response, so guard for their absence (API-key plans
# never get them). used_percentage is 0-100; resets_at is unix epoch seconds.
rate5=$(printf '%s' "$input" | jq -r '.rate_limits.five_hour.used_percentage // empty')
rate7d=$(printf '%s' "$input" | jq -r '.rate_limits.seven_day.used_percentage // empty')
reset5=$(printf '%s' "$input" | jq -r '.rate_limits.five_hour.resets_at // empty')
reset7d=$(printf '%s' "$input" | jq -r '.rate_limits.seven_day.resets_at // empty')
# compact "time until reset", rounded to the NEAREST unit so it lines up with
# the reset clock time shown in the app (flooring made big values read ~1h short).
# >=23h rolls up to whole days; 1h-23h -> nearest hour; under 1h -> nearest minute.
time_left() {
  local now left; now=$(date +%s); left=$(( $1 - now ))
  [ "$left" -le 0 ] 2>/dev/null && { printf '0m'; return; }
  if   [ "$left" -ge 82800 ]; then printf '%dd' $(( (left + 43200) / 86400 ))
  elif [ "$left" -ge 3600 ];  then printf '%dh' $(( (left + 1800) / 3600 ))
  else printf '%dm' $(( (left + 30) / 60 ))
  fi
}
if [ -n "$rate5" ] || [ -n "$rate7d" ]; then
  usage_txt="󰾆"
  if [ -n "$rate5" ]; then
    usage_txt+=" 5h $(printf '%.0f' "$rate5")%"
    [ -n "$reset5" ] && usage_txt+=" ($(time_left "$reset5"))"
  fi
  if [ -n "$rate7d" ]; then
    usage_txt+=" 7d $(printf '%.0f' "$rate7d")%"
    [ -n "$reset7d" ] && usage_txt+=" ($(time_left "$reset7d"))"
  fi
  # colour by the higher of the two percentages (mirrors context pressure)
  peak=$(awk "BEGIN{a=${rate5:-0}; b=${rate7d:-0}; printf \"%.0f\", (a>b?a:b)}")
  # dark text on the light colours, the snow's ink on the dark one
  usage_fg="$NIGHT"
  if   [ "$peak" -ge 90 ] 2>/dev/null; then usage_bg="$BAD"
  elif [ "$peak" -ge 75 ] 2>/dev/null; then usage_bg="$WARN"
  elif [ "$peak" -ge 50 ] 2>/dev/null; then usage_bg="$MIST"
  else usage_bg="$DUSK"; usage_fg="$INK"
  fi
  segment "$usage_bg" "$usage_fg" "$usage_txt"
fi

# --- Date + time (night) ------------------------------------------------------
segment "$NIGHT" "$INK" "  $(date '+%Y-%m-%d %H:%M')"

# closing cap
out+="$(fg "$prev_bg")$CAP_R$RESET"

printf '%s' "$out"

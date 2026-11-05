# tmite.zsh: ask a local model for a command from the zsh prompt.
#
# A convenience only: `tmite` does the same job without it. Type one of these
# and press Enter:
#
#   ? request    explains one command; say yes to put it at the prompt
#   ?? request   explains every command; pick one, ask again or cancel
#   ?x request   puts the command straight at the prompt if every check
#                passed, otherwise behaves like ?
#   ?b request   writes a script as a draft, and puts the command that shows
#                and installs it at the prompt
#
# termite never runs anything. A command only ever reaches the prompt, and
# pressing Enter is up to you.
#
# Load it from ~/.zshrc, after any plugin that wraps accept-line:
#
#   eval "$(tmite shell zsh)"
#
# Set TMITE_BIN if tmite is not on PATH.

: ${TMITE_BIN:=tmite}

# Keep the accept-line that was there before (another plugin may wrap it),
# once, so sourcing this file twice does not make the widget call itself.
if (( ! ${+widgets[_tmite_previous_accept_line]} )); then
  zle -A accept-line _tmite_previous_accept_line
fi

# Ask tmite, drawing progress while it works. Sets `output`. Progress lines
# come first; anything after the answer starts is the answer, never progress.
_tmite_ask() {
  local line fd
  output=
  zle -M "tmite: asking…"
  zle -R

  exec {fd}< <(command "$TMITE_BIN" --zle -- "$1" 2>&1 </dev/null)
  {
    while IFS= read -r -u $fd line; do
      if [[ -z $output && $line == progress$'\t'* ]]; then
        zle -M -- "tmite: ${line#*$'\t'}"
        zle -R
      else
        output+=$line$'\n'
      fi
    done
    # A last line with no line break, such as a usage error.
    [[ -n $line ]] && output+=$line
  } always {
    exec {fd}<&-
  }
  output=${output%$'\n'}
  # Clear the progress line from the screen now. Left for later, the prompt
  # is redrawn in the wrong place and covers what `_tmite_show` prints.
  zle -M ""
  zle -R
}

# Print above the prompt. Colour comes from tmite, so this prints as it is.
_tmite_show() {
  zle -I
  print -r -- "$1"
}

# Ask a question and read one key into `key`.
_tmite_key() {
  print -rn -- "$1 "
  # `zle -I` put the terminal back in line mode, where a key waits for Enter.
  stty -icanon min 1 time 0 </dev/tty 2>/dev/null
  read -k 1 key
  stty icanon </dev/tty 2>/dev/null
  print
}

# Yes to a question. Enter means yes, unless the command is risky.
_tmite_yes() {
  local key
  if [[ $1 == risky ]]; then
    _tmite_key "Risky. Put it at your prompt anyway? [y/N]"
    [[ $key == [yY] ]]
  else
    _tmite_key "Put it at your prompt? [Y/n]"
    [[ $key == [yY$'\r\n'] ]]
  fi
}

# Put a command at the prompt, in red if it is risky. It stays there until
# you press Enter yourself.
_tmite_insert() {
  BUFFER=$2
  CURSOR=$#BUFFER
  if [[ $1 == risky && -z ${NO_COLOR-} ]]; then
    region_highlight+=("0 $#BUFFER fg=red,bold")
  fi
}

_tmite_choose() {
  local count=$1 pick=
  local -a options=("${(@)lines[3,count + 2]}")
  local key

  _tmite_show "${(F)lines[count + 3,-1]}"
  _tmite_key "Press a number, a to ask again, any other key to cancel:"

  if [[ $key == [aA] ]]; then
    return 1
  fi
  if [[ $key == [1-9] ]] && (( key <= count )); then
    pick=$options[key]
    if [[ ${pick%%$'\t'*} != risky ]] || _tmite_yes risky; then
      _tmite_insert "${pick%%$'\t'*}" "${pick#*$'\t'}"
      return 0
    fi
  fi
  BUFFER=
  return 0
}

_tmite_accept_line() {
  # The request, without leading spaces.
  local query="${BUFFER#"${BUFFER%%[![:space:]]*}"}"

  # Only the red this file added; another plugin's highlights stay.
  region_highlight=("${(@)region_highlight:#0 * fg=red,bold}")

  if [[ $query != '?'* ]]; then
    zle _tmite_previous_accept_line
    return
  fi

  local output
  local -a lines

  while true; do
    _tmite_ask "$query"
    lines=("${(@f)output}")
    local kind=$lines[1] rating=$lines[2] command=$lines[3]

    # An older tmite has no rating line, so its lines mean something else here.
    # Line 3 would be its description, so nothing may reach the prompt.
    if [[ $kind == (confirm|straight_in) && $rating != (reads|changes|risky) ]]; then
      BUFFER=
      zle -M -- "tmite: this widget does not match the installed tmite. Nothing was put at your prompt. Update tmite, and then open a new shell."
      break
    fi

    case $kind in
      confirm)
        _tmite_show "${(F)lines[4,-1]}"
        if _tmite_yes "$rating"; then
          _tmite_insert "$rating" "$command"
        else
          BUFFER=
        fi
        ;;
      straight_in)
        _tmite_show "${(F)lines[4,-1]}"
        _tmite_insert "$rating" "$command"
        ;;
      refused)
        _tmite_show "${(F)lines[4,-1]}"
        BUFFER=
        ;;
      choose)
        # Line 2 is the count here, not a rating.
        _tmite_choose "$rating" || continue
        ;;
      no_command|failed)
        zle -M -- "${(F)lines[4,-1]}"
        ;;
      *)
        # Not the line format: tmite is missing, or printed a usage error.
        zle -M -- "${output:-tmite printed nothing}"
        ;;
    esac
    break
  done
}

zle -N accept-line _tmite_accept_line

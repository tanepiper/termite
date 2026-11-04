# shellcheck shell=bash
#
# termite confirm: ask before doing something that cannot be undone.
#
# Functions:
#   confirm QUESTION
#       Ask "QUESTION [y/N]" on the terminal. Returns 0 for y or yes, 1 for
#       anything else. With --yes it returns 0 without asking. Without a
#       terminal (cron, a pipe, CI) it warns and returns 1, so nothing
#       destructive happens unattended unless --yes was given.
#       Always use it in a condition:
#         confirm "Delete ${#files[@]} old backups?" || return 0

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  printf '%s is a library; source it from a script\n' "${0##*/}" >&2
  exit 64
fi
[[ -z ${_termite_confirm_loaded-} ]] || return 0
_termite_confirm_loaded=1

# Where the answer is read from. Tests point it at a file.
_confirm_input=/dev/tty

confirm() {
  local question=${1-} answer='' fd
  [[ -n $question ]] || die "confirm: give a question" 64
  ! args_on yes || return 0

  if ! { exec {fd}<"$_confirm_input"; } 2>/dev/null; then
    log_warn "no terminal to ask '$question'; answering no (use --yes)"
    return 1
  fi
  printf '%s [y/N] ' "$(log_printable "$question")" >&2
  IFS= read -r -u "$fd" answer || answer=''
  exec {fd}<&-

  answer=${answer,,}
  [[ $answer == y || $answer == yes ]]
}

# shellcheck shell=bash
#
# termite log: messages for people, on stderr, so stdout stays clean for
# data that other commands read.
#
# Functions (MESSAGE words are joined with spaces):
#   log_info MESSAGE      shown unless --quiet
#   log_verbose MESSAGE   shown with --verbose (-v)
#   log_debug MESSAGE     shown with -vv, prefixed "debug:"
#   log_warn MESSAGE      always shown: "NAME: warning: MESSAGE"
#   log_error MESSAGE     always shown: "NAME: error: MESSAGE"
#   log_printable TEXT    print TEXT on stdout with control characters
#                         replaced by "?", for file names and other
#                         untrusted text shown to a person
#
# Control characters in messages (except tab and new line) are replaced
# with "?", so a file name cannot send escape sequences to the terminal.
# Colour is used only when stderr is a terminal and NO_COLOR is not set.
#
# Variables:
#   TERMITE_LOG_LEVEL   0 quiet, 1 normal (default), 2 verbose, 3 debug;
#                       set by args_parse from -q / -v

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  printf '%s is a library; source it from a script\n' "${0##*/}" >&2
  exit 64
fi
[[ -z ${_termite_log_loaded-} ]] || return 0
_termite_log_loaded=1

declare -gi TERMITE_LOG_LEVEL=1

_log_clean() {
  local text=$1
  printf '%s' "${text//[$'\001'-$'\010'$'\013'-$'\037'$'\177']/?}"
}

_log_write() {
  local colour=$1 prefix=$2
  shift 2
  local IFS=' '
  local message
  message=$(_log_clean "$*")
  if [[ -t 2 && -z ${NO_COLOR-} && -n $colour ]]; then
    printf '\e[%sm%s%s\e[0m\n' "$colour" "$prefix" "$message" >&2
  else
    printf '%s%s\n' "$prefix" "$message" >&2
  fi
}

log_info() {
  ((TERMITE_LOG_LEVEL >= 1)) || return 0
  _log_write '' '' "$@"
}

log_verbose() {
  ((TERMITE_LOG_LEVEL >= 2)) || return 0
  _log_write '' '' "$@"
}

log_debug() {
  ((TERMITE_LOG_LEVEL >= 3)) || return 0
  _log_write 2 'debug: ' "$@"
}

log_warn() {
  _log_write 33 "$TERMITE_SCRIPT_NAME: warning: " "$@"
}

log_error() {
  _log_write 31 "$TERMITE_SCRIPT_NAME: error: " "$@"
}

log_printable() {
  _log_clean "${1-}"
}

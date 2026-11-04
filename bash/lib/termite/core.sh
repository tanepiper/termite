# shellcheck shell=bash
# shellcheck source-path=SCRIPTDIR
#
# termite core: strict mode, error reporting and exit handlers.
#
# Sourced by the termite preamble at the top of every termite script; it
# sources the other components (log, args, require, tmp, confirm) itself.
#
# What it sets up:
#   set -Eeuo pipefail, inherit_errexit   stop at the first failing command
#   IFS=$'\n\t'                           no splitting on spaces
#   umask 077                             new files are yours only
#   ERR trap                              reports the failing command, line
#                                         and status on stderr
#   INT / TERM traps                      exit 130 / 143, running exit handlers
#
# Functions:
#   die MESSAGE [STATUS]
#       Print "NAME: error: MESSAGE" on stderr and exit with STATUS
#       (default 1). Example: [[ -d $dir ]] || die "$dir is not a folder"
#
#   core_at_exit FUNCTION
#       Run FUNCTION when the script exits, for any reason. Handlers run
#       last-registered first; a failing handler does not stop the others.
#       Example: core_at_exit stop_server
#
# Variables:
#   TERMITE_SCRIPT_NAME   the script's file name, used in messages

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  printf '%s is a library; source it from a script\n' "${0##*/}" >&2
  exit 64
fi
[[ -z ${_termite_core_loaded-} ]] || return 0
_termite_core_loaded=1

set -Eeuo pipefail
shopt -s inherit_errexit
IFS=$'\n\t'
umask 077

TERMITE_SCRIPT_NAME=${0##*/}
declare -ga _termite_at_exit=()

die() {
  local status=${2:-1}
  [[ $status =~ ^[0-9]+$ ]] || status=1
  log_error "${1:-failed}"
  exit "$status"
}

core_at_exit() {
  local handler=${1-}
  if ! declare -F -- "$handler" >/dev/null; then
    die "core_at_exit: '$handler' is not a function" 64
  fi
  _termite_at_exit+=("$handler")
}

_termite_on_exit() {
  local status=$? i
  set +e
  trap - ERR
  for ((i = ${#_termite_at_exit[@]} - 1; i >= 0; i--)); do
    "${_termite_at_exit[i]}"
  done
  exit "$status"
}

_termite_on_signal() {
  trap - INT TERM
  exit "$1"
}

_termite_on_error() {
  local status=$1 line=$2 command=$3 file=${BASH_SOURCE[1]-}
  ((BASH_SUBSHELL == 0)) || return 0
  local where="line $line"
  if [[ -n $file && $file != "$0" ]]; then
    where+=" of ${file##*/}"
  fi
  log_error "$where: '$command' failed with status $status"
}

trap _termite_on_exit EXIT
trap '_termite_on_signal 130' INT
trap '_termite_on_signal 143' TERM
trap '_termite_on_error "$?" "$LINENO" "$BASH_COMMAND"' ERR

_termite_lib_dir=${BASH_SOURCE[0]%/*}
# shellcheck source=log.sh
source "$_termite_lib_dir/log.sh"
# shellcheck source=args.sh
source "$_termite_lib_dir/args.sh"
# shellcheck source=require.sh
source "$_termite_lib_dir/require.sh"
# shellcheck source=tmp.sh
source "$_termite_lib_dir/tmp.sh"
# shellcheck source=confirm.sh
source "$_termite_lib_dir/confirm.sh"

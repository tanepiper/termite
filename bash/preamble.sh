#!/usr/bin/env bash
# --- termite preamble 1: checks bash, then loads the termite library ---
if [ -z "${BASH_VERSINFO-}" ] || [ "${BASH_VERSINFO[0]}" -lt 5 ] ||
  { [ "${BASH_VERSINFO[0]}" -eq 5 ] && [ "${BASH_VERSINFO[1]}" -lt 3 ]; }; then
  if [ "$(uname -s)" = Darwin ]; then
    _termite_fix='brew install bash'
  else
    _termite_fix='install bash 5.3 or later with your package manager'
  fi
  printf '%s needs bash 5.3 or later; this is %s (%s).\nFix: %s\n' \
    "${0##*/}" "${BASH_VERSION:-not bash}" "${BASH:-sh}" "$_termite_fix" >&2
  exit 1
fi
_termite_load() {
  local home=${TERMITE_HOME:-$HOME/.config/termite} path problem version
  local lib=$home/lib/termite
  if [[ $home != /* ]]; then
    printf '%s: TERMITE_HOME must be an absolute path, not %s\n' \
      "${0##*/}" "$home" >&2
    exit 1
  fi
  for path in "$home" "$home/lib" "$lib" "$lib/VERSION" "$lib/core.sh" "$lib"/*.sh; do
    if [[ -L $path ]]; then
      problem='a symlink'
    elif [[ ! -e $path ]]; then
      problem='missing'
    elif [[ ! -O $path ]]; then
      problem='not owned by you'
    elif [[ -n $(find "$path" -prune \( -perm -020 -o -perm -002 \) -print) ]]; then
      problem='writable by other users'
    else
      continue
    fi
    printf '%s: cannot load the termite library: %s is %s.\nFix: run tmite setup\n' \
      "${0##*/}" "$path" "$problem" >&2
    exit 1
  done
  IFS= read -r version <"$lib/VERSION" || version=
  if [[ ! $version =~ ^[0-9]+$ ]] || ((version < 1)); then
    printf '%s: the termite library is version %s; this script needs 1 or later.\nFix: run tmite setup\n' \
      "${0##*/}" "${version:-unknown}" >&2
    exit 1
  fi
}
_termite_load
unset -f _termite_load
# shellcheck source=lib/termite/core.sh
source "${TERMITE_HOME:-$HOME/.config/termite}/lib/termite/core.sh"
# --- end of termite preamble ---

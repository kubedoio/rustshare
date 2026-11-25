# Opt in by sourcing this file from ~/.zshrc after installing the CLI.
autoload -Uz add-zsh-hook
zmodload zsh/datetime

_elembra_shell_memory_preexec() {
  typeset -g _ELEMBRA_SHELL_MEMORY_COMMAND="$1"
  typeset -g _ELEMBRA_SHELL_MEMORY_STARTED="$EPOCHREALTIME"
}

_elembra_shell_memory_precmd() {
  local status=$?
  if [[ -n "${_ELEMBRA_SHELL_MEMORY_COMMAND-}" ]]; then
    local duration_ms=$(( int((EPOCHREALTIME - _ELEMBRA_SHELL_MEMORY_STARTED) * 1000) ))
    printf '%s\0' "$_ELEMBRA_SHELL_MEMORY_COMMAND" "$PWD" "$HOST" "$status" "$duration_ms" |
      elembra-shell-memory record >/dev/null 2>&1 || print -u2 -- "Elembra Shell Memory: command was not queued"
  fi
  _ELEMBRA_SHELL_MEMORY_COMMAND=
}

add-zsh-hook preexec _elembra_shell_memory_preexec
add-zsh-hook precmd _elembra_shell_memory_precmd

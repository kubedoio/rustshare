# Opt in by sourcing this file from ~/.zshrc after installing the CLI.
autoload -Uz add-zsh-hook
zmodload zsh/datetime
export ELEMBRA_SHELL_MEMORY_SESSION_ID="$(elembra-shell-memory session-id 2>/dev/null)"

_elembra_shell_memory_preexec() {
  typeset -g _ELEMBRA_SHELL_MEMORY_COMMAND="$1"
  typeset -g _ELEMBRA_SHELL_MEMORY_STARTED="$EPOCHREALTIME"
}

_elembra_shell_memory_precmd() {
  local command_status=$?
  if [[ -n "${_ELEMBRA_SHELL_MEMORY_COMMAND-}" ]]; then
    local -i duration_ms
    (( duration_ms = (EPOCHREALTIME - _ELEMBRA_SHELL_MEMORY_STARTED) * 1000 ))
    printf '%s\0' "$_ELEMBRA_SHELL_MEMORY_COMMAND" "$PWD" "$HOST" "$command_status" "$duration_ms" "$ELEMBRA_SHELL_MEMORY_SESSION_ID" |
      elembra-shell-memory record >/dev/null 2>&1 || print -u2 -- "Elembra Shell Memory: command was not queued"
  fi
  _ELEMBRA_SHELL_MEMORY_COMMAND=
}

add-zsh-hook preexec _elembra_shell_memory_preexec
add-zsh-hook precmd _elembra_shell_memory_precmd

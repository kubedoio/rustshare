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
    local -a command_words
    command_words=( ${(z)_ELEMBRA_SHELL_MEMORY_COMMAND} )
    if [[ "${command_words[1]:t}" == "elembra-shell-memory" ]]; then
      local -i command_index=2
      while (( command_index <= ${#command_words} )); do
        case "${command_words[command_index]}" in
          --config) (( command_index += 2 )) ;;
          --config=*) (( command_index += 1 )) ;;
          *) break ;;
        esac
      done
      case "${command_words[command_index]-}" in
        hook|session-id|store-credential|record|run|pause|resume|disable|enable|forget|forget-last|forget-selected|forget-session|list|init-config)
          _ELEMBRA_SHELL_MEMORY_COMMAND=
          return 0
          ;;
      esac
    fi
    local -i duration_ms
    (( duration_ms = (EPOCHREALTIME - _ELEMBRA_SHELL_MEMORY_STARTED) * 1000 ))
    printf '%s\0' "$_ELEMBRA_SHELL_MEMORY_COMMAND" "$PWD" "$HOST" "$command_status" "$duration_ms" "$ELEMBRA_SHELL_MEMORY_SESSION_ID" |
      elembra-shell-memory record >/dev/null 2>&1 || print -u2 -- "Elembra Shell Memory: command was not queued"
  fi
  _ELEMBRA_SHELL_MEMORY_COMMAND=
}

add-zsh-hook preexec _elembra_shell_memory_preexec
add-zsh-hook precmd _elembra_shell_memory_precmd

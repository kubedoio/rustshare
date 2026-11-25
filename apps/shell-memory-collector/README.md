# Shell Memory collector

This opt-in zsh collector stores redacted command metadata in a local SQLite queue and publishes one private Files record per command through the existing Connector worker API. It never reads shell history, command output, or environment variables, and it does not execute remote commands.

Register `io.elembra.connector.shell-memory` through the normal Connector administration flow with `destinationFolderId` unset/null, issue its worker credential, install this CLI, then configure it. Imports go to the owner’s Files root; the collector never chooses a shared folder:

```sh
elembra-shell-memory init-config --server https://elembra.example --instance-id <connector-instance-uuid>
elembra-shell-memory store-credential < ~/.secure/shell-memory-worker-credential
```

The one-time credential is read from standard input and stored in the OS credential store. Never pass it as an argument. Add `eval "$(elembra-shell-memory hook zsh)"` to `~/.zshrc` to opt in, then run `elembra-shell-memory run` in a user service or terminal to upload queued records. Each upload round processes at most one command, then releases the lifecycle lock before the next round; successful rounds yield for 100 ms, while idle or failed rounds back off for 15 seconds. Capture holds only the short queue lock while writing. This bounds how long forget, credential rotation, pause, or disable can wait on an active upload. Each HTTP request has a 30-second timeout.

Add glob patterns to `exclude_commands` in the generated config to skip commands locally. Common secret arguments and assignments are redacted before the SQLite insert. Values with unlabelled names cannot be identified reliably; add exclusions for commands that may contain them. `pause` holds uploads while redacted records continue accumulating locally; `resume` restarts uploads. `disable` stops capture and upload, `enable` re-enables them, and `forget` deletes the local queue and credential while leaving collection disabled. To opt in again after `forget`, store a newly issued credential with `store-credential`, then run `enable`. Local `forget` does not delete imported Files records. To revoke the worker credential remotely, an authorized Connector owner must rotate the credential or disconnect the instance; the worker credential cannot administer its own instance.

shortcut: one Files object per command; switch to batched daily views when the per-command object count becomes material.

Records include the redacted command, completion time, working directory, host, exit code, and duration. Files remains the authority; normal Files permissions govern keyword search and citations. A retry reuses the queued source ResourceRef and content revision, so the Files import endpoint deduplicates it.

Use only synthetic commands for qualification. For a local self-check: `SQLX_OFFLINE=true cargo test -p shell-memory-collector`.

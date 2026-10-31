# Stock Buzz desktop acceptance procedure

This procedure uses the exact, unmodified `block/buzz-app` source at `662e91800af8204ec0e492522b8f23ef23b19baa`, its normal secure-storage implementation, and a disposable Phase 1 relay. Never use a production identity or relay.

## Prerequisites

- A graphical desktop session with an unlocked operating-system Secret Service/keychain. Use a disposable credential collection/profile where possible; verify its normal create/read/delete path before launching Buzz. Do not substitute a mock keyring or patch Buzz. If using a regular host keyring, remove the imported test credential through Buzz's normal UI and verify it is absent before leaving the session. If credential cleanup cannot be confirmed, stop before importing into that keyring.
- Relay and POC client from `scripts/upstream-buzz-poc/README.md`; start `run.sh` with `POC_HOLD=1` and save its printed disposable `nsec` only in the mode-0600 path inside its mode-0700 temporary directory.
- Built unmodified Buzz desktop binary at the pinned SHA. Record OS, display server, desktop environment, Buzz binary hash, Rust/Node/package-manager versions and source SHA/tree. Do not retain a desktop profile, keychain file, `nsec`, or raw logs.

## Manual assertions

1. In Buzz Settings, import the harness user's existing `nsec`. Verify the displayed public key equals `desktop_test_ready alice_pubkey=...`; do not record or photograph the secret input or reveal/export screen.
2. Configure the exact printed `wss://` relay and join its intended community. Confirm the existing identity remains selected and there is no second identity or unexpected community enrollment.
3. In another terminal, enter `SEND <unique text>` at the POC hold prompt. Confirm Buzz displays the same channel event and event ID from the POC JSON observation.
4. Send a unique channel message from Buzz. Enter `WAIT <unique text>` in the POC prompt and verify its event ID and author match the desktop event.
5. Exit Buzz normally, relaunch it using the same OS keychain, reconnect and verify the same public key and previous event. Enter `HISTORY <event_id>` to independently confirm relay history.
6. Use the desktop's native private-channel DM flow with Bob. Verify it discovers the `dm_channel` from `desktop_test_ready`. At the POC prompt, `SEND_DM <unique text>` sends a kind-9 message from Bob; confirm Buzz Desktop displays it with the same event ID. Send a separate message as Alice from the desktop, then enter `WAIT_DM <unique text>` and confirm the POC observation has the same event ID, Alice pubkey and DM channel ID. `HISTORY_DM <event-id>` verifies DM history. This test concerns NIP-29 hidden/private channels and kind-9 messages; it does not establish NIP-17/NIP-44 encrypted gift-wrap support.
7. Stop the POC and destroy only an explicitly disposable profile/keychain and the relay stack. If the regular host keyring was used, remove the Buzz test credential through the normal UI and verify it is absent; never delete the host keychain. Keep only sanitized output from the collector below; never submit raw terminal transcripts, screenshots containing credentials, private key files, keychain databases or desktop profile data.

## Sanitized evidence

Prepare a JSON object on stdin with only `relay_url`, expected/displayed/peer public keys, the public and DM channel IDs, boolean manual assertions, and the protocol-vs-desktop observations for each channel direction (each contains the channel ID, event ID and author pubkey observed at both ends). Do not include any field containing an `nsec`, secret, private key, password, token or raw application/session log. The collector rejects unknown fields and outputs only the public key, relay origin, test booleans, channel IDs, author public keys and event IDs:

```sh
node scripts/upstream-buzz-poc/desktop-evidence.mjs < sanitized-manual-observations.json > sanitized-desktop-evidence.json
```

`PASS_REVIEW_REQUIRED` only means the structured assertions are internally consistent. A human reviewer must inspect the session and sign off that the assertions were observed. Missing or false observations are `UNVERIFIED_OR_FAIL`; source inspection and a rendered GUI are not runtime acceptance. Store only the sanitized output as review evidence.

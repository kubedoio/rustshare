# PR Workflow Guide for Agents

## Branch naming

Use descriptive prefixes:

| Prefix                 | Purpose               |
| ---------------------- | --------------------- |
| `feat/description`     | New features          |
| `fix/description`      | Bug fixes             |
| `docs/description`     | Documentation changes |
| `refactor/description` | Code refactoring      |
| `test/description`     | Test changes          |
| `chore/description`    | Maintenance, deps, CI |

Example: `docs/public-preview-feedback-guidance`.

## DCO sign-off

Every commit must be signed:

```bash
git commit -s
```

## PR sections

Fill out the PR template, especially:

- **Summary** — what and why
- **Changes** — key files/behavior
- **Validation** — commands run and results
- **Risk / Safety Notes** — note any sensitive-area impact

## Safety checklist summary

Before submitting, confirm:

- [ ] No secrets, tokens, private URLs, customer data, or confidential logs included.
- [ ] Permission / visibility impact considered.
- [ ] Tests added or updated where behavior changed.
- [ ] Security note added for permission, indexing, connector, or RAG-related changes.
- [ ] Documentation updated where relevant.

## When to request human review

Request human review for changes touching:

- Permissions, access control, workspace visibility
- Indexing, search, RAG context boundaries
- Connectors, external imports, vault sync
- Storage backends, migrations, data compatibility
- Secret handling, authentication, cryptography

See [safety-boundaries.md](safety-boundaries.md) for the full list.

## Mandatory implementing-agent review loop

Opening a PR starts a required review loop for the implementing LLM agent. Do this for every PR, including documentation-only changes; a green CI run or a requested human review does not replace it.

1. Review the exact PR head after opening the PR. Read the complete diff and the surrounding callers, contracts, and tests needed to judge it. Check correctness, edge cases, error handling, security and data boundaries, architecture, compatibility, and whether the change is adequately tested.
2. Record actionable findings by severity and location. Fix every valid finding in code, tests, or documentation. Do not dismiss findings merely because the implementation is small or CI passes.
3. Run the focused checks affected by each fix, then commit and push the fix with DCO sign-off. Re-review the new exact head; findings can reveal additional issues.
4. Repeat the review, fix, validation, and re-review cycle until a fresh review of the current head finds no actionable issue. Update the PR description or add a concise PR comment with the reviewed head, checks run, and remaining non-actionable limitations.
5. Keep required CI and human/security approvals as separate merge gates. If an actionable finding cannot be resolved safely without an external dependency or authorized decision, document the blocker and leave the PR open; do not call the loop complete or merge it.

Apply this loop to findings from self-review, independent review, automated review, CI, and later PR feedback. After every head change, review that new head before requesting fresh external review or merging.

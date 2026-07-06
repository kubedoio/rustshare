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

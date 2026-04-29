# Getting Support for RustShare

## Support Channels

| Channel | Use For | Where |
|---------|---------|-------|
| **GitHub Issues** | Bug reports, feature requests, regressions | [github.com/kubedoio/rustshare/issues](https://github.com/kubedoio/rustshare/issues) |
| **GitHub Discussions** | Questions, configuration help, best practices, general chat | [github.com/kubedoio/rustshare/discussions](https://github.com/kubedoio/rustshare/discussions) |

### Choosing the Right Channel

- **Open an Issue** when something is broken, a documented feature does not work, or you have a reproducible crash or error.
- **Start a Discussion** when you need help setting something up, want advice on architecture or configuration, or are unsure whether something is a bug.

## Response Times

| Type | Target |
|------|--------|
| New issues triaged and labeled | Within 48 hours |
| Critical bugs (data loss, security-adjacent crashes) | Within 24 hours |
| Discussion questions | When possible — no SLA, but the maintainers monitor the forum |

These are best-effort targets for the core team. The project is pre-1.0 and run by a small team, so response times may vary around releases and holidays.

## Before Opening an Issue

Please run through this checklist first. It helps us resolve your problem faster:

- [ ] I have searched existing issues and discussions to avoid duplicates.
- [ ] I am running the latest commit on `main` or the most recent release tag.
- [ ] I have checked [docs/troubleshooting.md](docs/troubleshooting.md) for my symptoms.
- [ ] I have gathered the relevant logs:
  - `docker compose logs backend --tail 200`
  - `docker compose logs nginx --tail 100`
  - `docker compose logs postgres --tail 50`
- [ ] I can reproduce the problem with the steps I am about to provide.
- [ ] I have run `./scripts/final-launch-smoke.sh` and noted the output.

### Information to Include

A good issue or discussion post includes:

1. **What you expected to happen**
2. **What actually happened** (with full error messages or screenshots)
3. **Steps to reproduce** (minimal and specific)
4. **Environment details:**
   - RustShare version or git commit
   - `docker compose version` output
   - Host OS and version
   - Browser (if frontend-related)
5. **Relevant configuration** (redact secrets and passwords)

## Security Issues

**Do not open public issues for security vulnerabilities.**

See [SECURITY.md](SECURITY.md) for responsible disclosure instructions, including how to submit a private security advisory and expected timelines.

## Troubleshooting

For common problems and fixes—database connection failures, upload issues, authentication errors, and more—see the [Troubleshooting Guide](docs/troubleshooting.md).

## Contributing

If you want to fix a bug or add a feature yourself, see [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, test commands, and contribution guidelines.

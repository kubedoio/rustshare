# RustShare Roadmap

This document outlines the public roadmap for RustShare. It is a living document that reflects our current priorities and planned direction.

## Current Status (Phase 1)

RustShare is in late MVP / pre-release. For a detailed snapshot of current maturity, see [docs/STATUS.md](docs/STATUS.md) and [docs/PRODUCTION_READINESS.md](docs/PRODUCTION_READINESS.md).

The project currently focuses on:

- **Web file-sharing** — the most mature component, approximately 94-96% complete
- **Secure sharing and permissions** — role-based access controls and share links
- **Admin panel and configuration** — web-based administration and settings
- **Docker-based deployment** — containerized backend and frontend deployment

## In Progress

The following items are actively being worked on:

- Frontend polish and responsive design
- OIDC production validation
- Real-world restore drill validation
- Replication alerting

## Near Term (Next 3-6 months)

- Stable v1.0 release with SemVer versioning
- TLS/HTTPS deployment guides
- Complete CI/CD hardening
- Security audit
- Mobile responsiveness improvements

## Medium Term (6-12 months)

- Kubernetes / Helm deployment options
- SCIM v2 provisioning support
- Desktop sync client productionization
- Enterprise features (audit logs, SSO enhancements)

## Long Term

- Mobile applications
- AI-assisted content features (permission-aware)
- Federation between RustShare instances

## Deferred / Postponed

- **Mobile native apps** — postponed until the web experience is stable and v1.0 has shipped

## How to Influence the Roadmap

We welcome community input. You can influence the roadmap by:

- Opening a feature request issue
- Joining discussions on existing issues and pull requests
- Contributing code, documentation, or design work

For detailed implementation specifications, see the plans in [docs/plans/](docs/plans/) and [docs/superpowers/plans/](docs/superpowers/plans/).

# Security Policy

This document outlines the security policy for **RustShare**, a self-hosted file-sharing and sync platform.

## Supported Versions

RustShare is currently pre-1.0. Only the latest commit on `main` and the most recent tagged release receive security updates.

| Version | Supported |
|---------|-----------|
| Latest `main` | ✅ |
| Latest release tag | ✅ |
| Older tags | ❌ |

## Reporting a Vulnerability

We take security vulnerabilities seriously. If you discover a security issue, please report it using one of the following methods:

### 1. GitHub Private Security Advisories (Preferred)

Submit a private security advisory at:  
**[https://github.com/kubedoio/rustshare/security/advisories/new](https://github.com/kubedoio/rustshare/security/advisories/new)**

This is the preferred method as it allows for confidential discussion and coordinated disclosure directly within the project repository.

### 2. Email Fallback

If you are unable to use GitHub Security Advisories, you may send an email to:  
**security@rustshare.io**

Please include "[SECURITY]" in the subject line.

### Expected Timeline

Once a vulnerability report is received, you can expect the following:

- **Acknowledgment** within 48 hours
- **Initial assessment** within 7 days
- **Fix and disclosure** within 90 days of acknowledgment (standard coordinated disclosure)
- If the reporter requests a shorter timeline, the project will try to accommodate

### What to Include in a Report

To help us triage and resolve issues quickly, please include:

- A clear description of the vulnerability
- Step-by-step instructions to reproduce the issue
- The potential impact (e.g., data exposure, unauthorized access, denial of service)
- A suggested fix or mitigation, if you have one

## Disclosure Policy

RustShare follows **coordinated disclosure**:

- The reporter will be kept informed of progress throughout the investigation.
- The reporter will be credited publicly in the advisory unless they request anonymity.
- After a fix is released, a public security advisory will be published.

## Security-Related Configuration

When deploying RustShare, please observe the following security practices:

- Run `scripts/pre-flight.sh` before deployment to generate strong, random secrets.
- Never use the default values from `.env.example` in production.
- Keep dependencies updated via Dependabot.
- Monitor [RustSec](https://rustsec.org/) advisories for security issues in Rust dependencies.

## Scope

The following are **in scope** for security reports:

- Backend server
- Frontend application
- Desktop client
- Docker images
- Deployment scripts

The following are **out of scope**:

- Third-party infrastructure misconfiguration (unless it results from a documented RustShare default)

# Restore Drill Checklist

Use this checklist for a monthly or pre-release restore drill.

## Inputs

- a recent backup directory from `scripts/backup-stack.sh`
- a disposable Docker host or isolated local environment
- expected validation sample:
  - one known admin login
  - one known file
  - one known public share

## Drill Steps

1. Verify the bundle before restore:

```bash
scripts/verify-backup-bundle.sh backups/<timestamp>
```

2. Restore the bundle into the disposable environment:

```bash
scripts/restore-stack.sh backups/<timestamp>
```

3. Verify platform health:

```bash
docker compose ps
curl -f http://localhost/health
curl -f http://localhost:8080/health
```

4. Verify application behavior:
- log in with a known admin account
- browse the main file list
- download a known file
- open a known public share
- confirm RustFS still serves expected objects

5. Record the drill:
- backup timestamp used
- git commit in `manifest.env`
- restore duration
- validation result
- any repair steps needed

## Exit Criteria

The drill passes only if:
- restore completes without manual DB or object-store surgery
- backend and nginx return healthy responses
- at least one authenticated and one public file-sharing flow works

## Current Follow-Ups

This checklist still needs:
- a dedicated disposable restore environment template
- a scripted post-restore smoke test for login/upload/share/download

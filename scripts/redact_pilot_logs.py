#!/usr/bin/env python3
"""Redact configured pilot credentials from byte-oriented diagnostic streams."""

from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path
from typing import Mapping, Sequence

SECRET_ENV_VARS = (
    "JWT_SECRET",
    "RUSTSHARE_SECRET_ENCRYPTION_KEY",
    "RUSTSHARE_CHAT_WEBHOOK_SECRET",
    "RUSTSHARE_ADMIN_PASSWORD",
    "RUSTSHARE_DEMO_VIEWER_PASSWORD",
    "POSTGRES_PASSWORD",
    "DATABASE_URL",
    "RUSTFS_ROOT_USER",
    "RUSTFS_ROOT_PASSWORD",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
)
REDACTED = b"[REDACTED]"
SECRET_FOUND = 10
SENSITIVE_PATTERNS = (
    re.compile(
        rb"(?im)(authorization\s*[:=]\s*bearer\s+)(?!\[REDACTED\])[A-Za-z0-9._~+/=-]+"
    ),
    re.compile(rb"(?im)((?:set-cookie|cookie)\s*[:=]\s*)(?!\s*\[REDACTED\])[^\r\n]+"),
    re.compile(rb"(?i)\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b"),
)


def secret_values(environ: Mapping[str, str]) -> tuple[bytes, ...]:
    return tuple(
        sorted(
            {
                environ[name].encode("utf-8", errors="surrogateescape")
                for name in SECRET_ENV_VARS
                if environ.get(name)
            },
            key=len,
            reverse=True,
        )
    )


def missing_secret_names(environ: Mapping[str, str]) -> tuple[str, ...]:
    return tuple(name for name in SECRET_ENV_VARS if not environ.get(name))


def redact_bytes(data: bytes, secrets: Sequence[bytes]) -> tuple[bytes, bool]:
    """Return redacted bytes and whether any configured secret was present."""
    # bytes.replace treats the secret literally (including regex/sed syntax,
    # backslashes, and newlines); longest-first also handles overlapping values.
    secrets = tuple(secret for secret in set(secrets) if secret)
    found = any(secret in data for secret in secrets)
    for secret in sorted(secrets, key=len, reverse=True):
        data = data.replace(secret, REDACTED)
    for pattern in SENSITIVE_PATTERNS:
        if pattern.search(data):
            found = True
            if pattern.groups:
                data = pattern.sub(lambda match: match.group(1) + REDACTED, data)
            else:
                data = pattern.sub(REDACTED, data)
    return data, found


def tree_contains_secret(root: Path, secrets: Sequence[bytes]) -> bool:
    if root.is_symlink() or not root.is_dir():
        raise OSError("evidence directory is missing")

    def raise_walk_error(error: OSError) -> None:
        raise error

    for directory, subdirectories, filenames in os.walk(
        root, onerror=raise_walk_error, followlinks=False
    ):
        directory_path = Path(directory)
        for name in subdirectories + filenames:
            if (directory_path / name).is_symlink():
                raise OSError("evidence tree contains a symbolic link")
        for name in filenames:
            data = (directory_path / name).read_bytes()
            if any(secret and secret in data for secret in secrets) or any(
                pattern.search(data) for pattern in SENSITIVE_PATTERNS
            ):
                return True
    return False


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--require-all", action="store_true")
    parser.add_argument("--check-tree", type=Path)
    args = parser.parse_args(argv)

    environ = os.environ
    secrets = secret_values(environ)
    if args.require_all and missing_secret_names(environ):
        print("Pilot evidence redaction is missing required secret values.", file=sys.stderr)
        return 2

    try:
        if args.check_tree is not None:
            found = tree_contains_secret(args.check_tree, secrets)
        else:
            data, found = redact_bytes(sys.stdin.buffer.read(), secrets)
            sys.stdout.buffer.write(data)
    except Exception:
        # Exception details may contain input data, including the secret itself.
        print("Pilot evidence redaction or verification failed.", file=sys.stderr)
        return 2

    if found:
        print("A configured secret was found and redacted from diagnostics.", file=sys.stderr)
        return SECRET_FOUND
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

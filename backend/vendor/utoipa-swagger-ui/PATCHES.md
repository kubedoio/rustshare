# Local patch

This is `utoipa-swagger-ui` 9.0.2 from crates.io, patched because its published
build dependency pins vulnerable `zip` 3.x and its build script applies archive
Unix permission bits without masking SUID, SGID, and sticky bits.

- The build dependency uses `zip` 9.x.
- Extracted file modes are restricted to `0o777`.

Rebase or remove this copy when upstream publishes equivalent fixes. Preserve
the permission mask if rebasing the build script.

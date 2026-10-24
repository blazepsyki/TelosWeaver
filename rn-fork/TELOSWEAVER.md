# TelosWeaver Fork of Reason Network

This directory is a fork of [Reason Network](https://github.com/reason-machines/reason-network), maintained as part of
TelosWeaver (Spec v1.5 §12, DEC-07). The original license files (`LICENSE-MIT`, `LICENSE-APACHE`) and `NOTICE` are kept
unchanged. Files modified by TelosWeaver are listed in the patch inventory below.

## Upstream base

| Item | Value |
|---|---|
| Upstream repository | https://github.com/reason-machines/reason-network |
| Upstream tag | `v0.12.11` |
| Upstream commit | `b3557ff454cd26a04c2a18804ba83ae5db8b119d` |
| Import method | `git subtree add --prefix=rn-fork <upstream> v0.12.11 --squash` (2026-10-25) |
| Lockfile | Upstream `Cargo.lock` kept as imported (446 packages) |
| Workspace | Independent Cargo workspace, excluded from the TelosWeaver root workspace |

## Fork policy (DEC-07)

Removed from this fork:

1. Integration with the closed-source `reason` CLI (session listing and delivery, CLI-managed install mode, `__relay`).
2. Automatic updates and download-and-run installation (update checks, install scripts, self-replacement, restart).
3. Reason account sign-in (sign-in proof verification and verified names).

Kept: local daemon, SQLite store, local IPC, Claude Code / Codex session discovery and delivery, MCP server and hooks
(applied only when the user runs `setup` explicitly), network / relay / team / invite features. Automatic login-service
registration is not used. The binary name, state directory, MCP server name and environment variable prefix will be
changed so this fork never shares state with an official Reason Network install.

Upstream changes are adopted selectively: review the upstream diff, apply only the needed fixes, then repeat the removal
checks so removed paths are not reintroduced. Full upstream merges and the upstream auto-update are not used.

## Patch inventory

| # | TelosWeaver task | Change | Files |
|---|---|---|---|
| — | PH0-RN-01 | Imported unmodified upstream `v0.12.11`; added this file | `TELOSWEAVER.md` |

Planned (see `docs/phase-0/evidence/PH0-RN/RN-00/README.md` in the TelosWeaver repository):

- PH0-RN-02: remove `src/daemon/update.rs`, `src/migrate.rs`, `install.sh`, `install.ps1`, `tests/install_script.rs`,
  `installer()` / `install_lines()`, `auto_update`, and login-service registration in `src/setup.rs`.
- PH0-RN-03: remove `src/daemon/reason.rs`, `managed()`, `cfg.reason`, the `reason` agent; remove `src/proof.rs`,
  `Request::Proof`, the `proof` / `endpoint` commands, verified names, and the `ring` dependency.
- PH0-RN-04: rename binary / state / MCP server / environment variables; fetch the relay list only when in a team.

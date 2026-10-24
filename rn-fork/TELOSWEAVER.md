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
| 1 | PH0-RN-02 | Removed the update loop and the `upgrade` command (download-and-run installer) | `src/daemon/update.rs` (deleted), `src/daemon/mod.rs`, `src/main.rs` |
| 2 | PH0-RN-02 | Removed self-replacement / tincan move-over at start (`migrate`) | `src/migrate.rs` (deleted), `src/main.rs`, `src/daemon/mod.rs`, `src/setup.rs` (`legacy_wiring`), `src/util.rs` (`from_source`) |
| 3 | PH0-RN-02 | Removed install scripts and their test; invite output shows the code and `join` only (no install command) | `install.sh`, `install.ps1`, `tests/install_script.rs` (deleted), `src/main.rs` (`install_lines`, `installer`, `REPO`), `src/wizard.rs` |
| 4 | PH0-RN-02 | Removed `auto_update` (an old config key is ignored when read) | `src/config.rs` |
| 5 | PH0-RN-02 | Removed login-service registration (launchd / systemd `--user` / Windows Run key): `setup` has no `--no-service`, `doctor` has no service line, clients always start the daemon on demand, the tray runs only under `daemon --service` started by hand | `src/setup.rs`, `src/client.rs`, `src/daemon/mod.rs`, `src/wizard.rs`, `src/main.rs`, `src/config.rs`, `src/tray.rs` |
| 6 | PH0-RN-02 | Old-peer refusal text no longer points to `reason update` | `src/daemon/mail.rs` |
| 7 | PH0-RN-03 | Removed the Reason CLI session sync and delivery (`reason session list` / `send`), the `__relay` child, and its push backoff | `src/daemon/reason.rs` (deleted), `src/daemon/mod.rs`, `src/daemon/mail.rs`, `src/daemon/sessions.rs` |
| 8 | PH0-RN-03 | Removed the `reason` agent (detection, label, setup entry) and `cfg.reason` (an old config key is ignored when read) | `src/agent.rs`, `src/setup.rs`, `src/config.rs`, `src/render.rs` (`for_agent` loses its `agent` parameter), `src/wizard.rs` |
| 9 | PH0-RN-03 | Removed the CLI-managed install mode (`managed()`): `CLI` and `ENGINE` are always `reason-network` | `src/main.rs`, `src/mcp.rs` |
| 10 | PH0-RN-03 | Removed Reason sign-in proofs: `proof` / `endpoint` commands, `Request::Proof`, `proof` on `CreateTeam` / `Join` and on the join protocol (`JoinRequest.proof`, `owner_only`), team-state `attestations`, verified names (`MachineInfo.verified`, `has_proof`), the same-account join path (`same_account_needs_owner`). Names in a team are unverified; joining is by invite code or team link only | `src/main.rs`, `src/proto.rs`, `src/client.rs`, `src/daemon/mod.rs`, `src/daemon/teamops.rs`, `src/team.rs`, `src/net.rs`, `src/render.rs`, `src/wizard.rs` |
| 11 | PH0-RN-03 | Split the JWS check that the relay list uses out of `proof.rs` into `signed.rs` (same pinned key, same rules); the test keys moved to `src/testdata/signer.pk8` / `forger.pk8`. `ring` stays for it | `src/proof.rs` → `src/signed.rs`, `src/relays.rs`, `src/testdata/*` |
| 12 | PH0-RN-03 | Agent skill text no longer sends users to sign in on reasonmachines.com; comments no longer name the Reason CLI | `src/skill.md`, `src/proto.rs`, `src/daemon/mail.rs` |

Upstream `README.md`, `DESIGN.md` and `tests/e2e/*.py` still describe upstream behavior (install scripts, service,
sign-in proofs; `tests/e2e/proofs.py` also holds the relay-list signing helpers the other scripts import); they are
rewritten or dropped with the rename in PH0-RN-04.

Planned (see `docs/phase-0/evidence/PH0-RN/RN-00/README.md` in the TelosWeaver repository):

- PH0-RN-04: rename binary / state / MCP server / environment variables; fetch the relay list only when in a team;
  remove the tincan legacy handling (`LEGACY_NAME`); make `init` / `join` stop wiring agents unless `setup` is run.

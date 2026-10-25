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
| Lockfile | Upstream `Cargo.lock` as imported (446 packages); only this package's own name changed (PH0-RN-04) |
| Workspace | Independent Cargo workspace, excluded from the TelosWeaver root workspace |

## Fork policy (DEC-07)

Removed from this fork:

1. Integration with the closed-source `reason` CLI (session listing and delivery, CLI-managed install mode, `__relay`).
2. Automatic updates and download-and-run installation (update checks, install scripts, self-replacement, restart).
3. Reason account sign-in (sign-in proof verification and verified names).

Kept: local daemon, SQLite store, local IPC, Claude Code / Codex session discovery and delivery, MCP server and hooks
(applied only when the user runs `setup` explicitly), network / relay / team / invite features. Automatic login-service
registration is not used. The binary name, state directory, MCP server name and environment variable prefix are
`telos-rn` (`~/.telos-rn`, `TELOS_RN_*`), so this fork never shares state with an official Reason Network install.
Team links are the bare `rn1-…` string. Wire identifiers (ALPNs `tincan/1` / `tincan/join/1`, the entry-signing
context, invite key derivation, the `rn1-` ticket format) and the relay list's signed format stay as upstream's, as
upstream's CONTRIBUTING.md requires; user-facing text still calls the product "Reason Network".

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
| 13 | PH0-RN-04 | Renamed the package and binary to `telos-rn`; `AGENT_NAME` (MCP server, hook, plugin and extension names) is `telos-rn`; environment variables are `TELOS_RN_*` only (no `REASON_NETWORK_*` / `TINCAN_*` fallback); config backups are `<file>.bak-telos-rn`; the macOS tray autosave name is `telos-rn.tray` | `Cargo.toml`, `Cargo.lock`, `src/main.rs`, `src/paths.rs`, `src/setup.rs`, `src/setup/zcode.rs`, `src/daemon/ask.rs`, `src/daemon/notify.rs`, `src/tray.rs`, `src/*.ts`, `src/skill.md` and comments / test names throughout |
| 14 | PH0-RN-04 | State in `~/.telos-rn` (or `TELOS_RN_HOME`), pipe `\\.\pipe\telos-rn-<hash>`, short socket dir `/tmp/telos-rn-<uid>`; removed the move-over of `~/.tincan` / `tincan.db` / the legacy socket link | `src/paths.rs` (rewritten), `src/main.rs` |
| 15 | PH0-RN-04 | Removed the tincan (`LEGACY_NAME`) wiring clean-up in `setup` (old MCP servers, hook / plugin / extension files, old backups, the old binary beside this one) | `src/setup.rs`, `src/main.rs` |
| 16 | PH0-RN-04 | Team links are the bare ticket (`join_url` returns it; `SITE_URL` removed; decoding still takes a whole upstream link); the invite line is `Join the telos-rn team: <link>`; dropped the "send it to your cloud agent" hint | `src/main.rs`, `src/proto.rs`, `src/wizard.rs`, `src/skill.md` |
| 17 | PH0-RN-04 | `init` / `join` (and the guided flow) no longer wire agents in: `--no-setup`, `setup::quietly` and `set_up_for` in `--json` are gone; they print how to run `setup` | `src/main.rs`, `src/wizard.rs`, `src/setup.rs` |
| 18 | PH0-RN-04 | The relay list is fetched only by a machine in a team that uses Reason's relays (never in no team) | `src/daemon/mod.rs`, `src/relays.rs` |
| 19 | PH0-RN-04 | New `README.md` for the fork; fork banners on upstream `DESIGN.md`, `CONTRIBUTING.md`, `SECURITY.md`; `tests/e2e/README.md` marks the upstream scripts unmaintained; removed the upstream banner image | `README.md`, `DESIGN.md`, `CONTRIBUTING.md`, `SECURITY.md`, `tests/e2e/README.md`, `assets/reason-network-banner.png` (deleted) |
| 20 | PH0-RN-05 | Every stored message has a TelosWeaver receipt (`receipts`, schema 1 via `PRAGMA user_version`): `delivered` only once the recipient was shown it (inbox, hook, `Delivered`) or its transcript shows it; handed to an agent's own inbox or queue it stays `queued` (`Delivered.handed_off`, transcript watch); expiry (at most 7 days, default 7 days from storage), final receipts and their messages deleted 7 days on, failed when the session is forgotten, refused / queued when the person drops / approves | `src/store.rs`, `src/daemon/mail.rs`, `src/daemon/mod.rs`, `src/daemon/sessions.rs`, `src/proto.rs`, `src/mcp.rs`, `src/hook.rs`, `src/main.rs` |
| 21 | PH0-RN-05 | Mail for a session by key isn't given to another session that takes its address: it is held for the person (`identity_mismatch`); approving it gives it to whoever has the address. Mail queued for an offline non-kept session is no longer handed to the next session in its place automatically | `src/daemon/mail.rs`, `src/store.rs` |
| 22 | PH0-RN-05 | TelosWeaver endpoints (host id from `TELOS_RN_HOST_ID`, provider, the agent's session id; fingerprint without the folder) and envelopes: `send_envelope`, `receipt`, `endpoint` local requests with idempotency keys per sender, contract error codes in replies (`code`), hidden `telos-rn call` for testing. The contract types come from `telos-schema` (path dependency; `Cargo.lock` gains it only) | `src/telos.rs` (new), `src/daemon/envelope.rs` (new), `src/proto.rs`, `src/main.rs`, `Cargo.toml`, `Cargo.lock` |
| 23 | PH0-RN-05 | Before a schema migration the database is copied to `backups/` (last 3 kept); a database from a newer schema is refused | `src/store.rs` |
| 24 | PH0-RN-05 | `codex queue` uses only the Codex CLI Core passes as `TELOS_RN_CODEX_BIN` (DEC-06), never one on `PATH`; without it Codex sessions get mail through hooks only | `src/agent.rs`, `src/daemon/mail.rs`, `src/daemon/sessions.rs` |
| 25 | PH0-RN-06 | TelosWeaver RPC (`telos_schema::rn`, contract `7497877`) on its own pipe / socket beside the agents' one: `rpc.json` (where), `rpc-token` (new at each start, this user's only), `rn.hello` first; `health.get` (fork commit from `build.rs`), `providers.list`, `sessions.discover` / `resolve` / `capabilities`, `messages.send` / `status` / `subscribe` (events: receipts of Core's messages, messages to Core's mailbox, presence; last 1,024 in memory, `resync_required` for a cursor they don't reach or from before a restart), `providers.agy.probe` refused; 1 MiB frames. Hidden `telos-rn rpc` for testing replaces RN-05's `call` and its interim local-API requests | `src/daemon/rpc.rs` (new), `src/rpccli.rs` (new), `build.rs` (new), `src/daemon/mod.rs`, `src/daemon/envelope.rs`, `src/proto.rs`, `src/main.rs`, `src/paths.rs`, `src/telos.rs`, `src/store.rs` |
| 26 | PH0-RN-06 | Both pipes and the RPC token are this OS user's only: a protected DACL with the current user's SID (as TelosWeaver Core's Local API), remote clients rejected; Unix sockets and the token 0600. Without it the RPC doesn't open | `src/private.rs` (new), `src/ipc.rs`, `Cargo.toml` (`windows-sys` features) |
| 27 | PH0-RN-06 | Sessions remember whether they came in through telos-rn (its MCP server or their own hooks), not only Claude Code's registry (`sessions.wired`, schema 2). Core sends only to those; others are listed unattached and refused | `src/daemon/mod.rs`, `src/daemon/sessions.rs`, `src/daemon/envelope.rs`, `src/store.rs` |
| 28 | PH0-RN-06 | Core's mailbox `telosweaver` (provider `telosweaver`), kept for good: the sender of Core's envelopes, where agents' replies land; not announced to teammates | `src/daemon/mod.rs`, `src/daemon/rpc.rs`, `src/daemon/directory.rs` |
| 29 | PH0-RN-06 | Fixed a Unix-only build break from PH0-RN-04: `ipc.rs` still called the removed `paths::link_legacy_socket` | `src/ipc.rs` |

Upstream `tests/e2e/*.py` still use upstream names and removed features; whether to port or drop each is decided in
PH0-RN-08.

Planned (see `docs/phase-0/evidence/PH0-RN/RN-00/README.md` in the TelosWeaver repository):

- PH0-RN-07: `telos-rn-client` and Core starting the engine (`TELOS_RN_HOME`, `TELOS_RN_HOST_ID`, `TELOS_RN_CODEX_BIN`).

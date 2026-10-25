# telos-rn

`telos-rn` is the TelosWeaver fork of [Reason Network](https://github.com/reason-machines/reason-network)
(`v0.12.11`): one small daemon per computer that lists the coding agent sessions on a team's machines (Claude Code,
Codex and others) and carries messages between them, end-to-end encrypted and peer to peer.

What this fork changes, and why, is in [TELOSWEAVER.md](TELOSWEAVER.md). In short:

- No closed-source `reason` CLI integration, no automatic updates or download-and-run installers, no Reason account
  sign-in, no login service.
- Its own names: the `telos-rn` binary, state in `~/.telos-rn` (or `TELOS_RN_HOME`), the `telos-rn` MCP server and
  `TELOS_RN_*` environment variables. It never shares or moves state with an official Reason Network install.
- Team links are the bare `rn1-…` string, with no web address.
- `init` and `join` don't wire agents in; only `telos-rn setup` does (`--dry-run` shows what it would change,
  `--remove` undoes it).
- A machine in no team starts no network and fetches nothing.
- Messages have TelosWeaver receipts: `delivered` means the recipient was shown the message, not that it was handed
  to the agent's inbox; mail for a session never goes to another that takes its address without the person's approval.
- `codex queue` runs only the Codex CLI given as `TELOS_RN_CODEX_BIN`, never one on `PATH`.
- TelosWeaver Core talks to it through its own RPC (`rpc.json`, `rpc-token` in the state folder), not the agents'
  socket; both are this user's only. Core reaches only sessions attached through telos-rn.

## Use

```sh
telos-rn init <team> --as <name>        # start a team
telos-rn link                           # the invite line for teammates
telos-rn join '<rn1-…>' --as <name>     # join with a team link (or a four-word code)
telos-rn setup --dry-run                # what wiring agents in would change
telos-rn who                            # people, machines and sessions
telos-rn doctor                         # checks
```

## Build

From this directory (an independent Cargo workspace, excluded from the TelosWeaver root workspace):

```sh
cargo fmt && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked
```

Run the tests with `USERPROFILE`/`HOME`, `APPDATA`, `LOCALAPPDATA` and `TELOS_RN_HOME` pointed at a temporary folder, so
they never touch your own agents' settings. The Python scripts in `tests/e2e/` are upstream's and are not maintained
in this fork yet (see TELOSWEAVER.md).

`DESIGN.md`, `CONTRIBUTING.md` and `SECURITY.md` are upstream's documents, kept for reference.

## License

MIT OR Apache-2.0, as upstream: [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE), [NOTICE](NOTICE).

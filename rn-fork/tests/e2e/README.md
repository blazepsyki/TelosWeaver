# Upstream E2E scripts (not maintained in this fork yet)

These Python scripts are upstream Reason Network's and still use its names (`reason-network`, `REASON_NETWORK_*`) and
features this fork removed: Reason sign-in proofs (`proofs.py`, which also holds the relay-list signing helpers the
others import, `names.py`, `two_machines.py`), an older upstream build (`mixed_versions.py`), `--no-setup` and the
login service. They are not run in TelosWeaver Phase 0. Whether to port or drop each one is decided in PH0-RN-08 (see
`../../TELOSWEAVER.md`).

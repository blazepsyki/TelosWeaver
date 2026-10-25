# Upstream E2E scripts (dropped in PH0-RN-08)

The upstream Python E2E scripts were dropped in PH0-RN-08 (2026-10-26) by user decision.

### Rationale
- The upstream scripts depended on removed features: Reason sign-in proofs (`proofs.py`), Reason CLI integration, automatic updates, login-service registration, legacy naming (`reason-network`), and external network services.
- TelosWeaver's acceptance and regression test suite replaces them with:
  1. Isolated Rust tests in `rn-fork` (204 tests on Windows, 212 tests on Linux) covering stores, envelopes, receipts, IPC DACL/permissions, and RPC protocols.
  2. `telos-rn-client` contract and protocol tests (`crates/rn-client/tests/client.rs`, 6 tests).
  3. Core integration tests (`crates/core/tests/phase0_rn.rs`, 3 offline tests + live integration tests).
  4. Live daemon round-trip, restart, and fault-independence verification under TelosWeaver Core supervisor.

> **TelosWeaver fork:** this is the upstream Reason Network document, kept for reference. It describes upstream
> behavior (the `reason-network` names, the Reason CLI, sign-in, updates, install scripts, the login service), which
> this fork removed or renamed; see [TELOSWEAVER.md](TELOSWEAVER.md).

# Security

Please report vulnerabilities privately through
[GitHub's private reporting](https://github.com/reason-machines/reason-network/security/advisories/new), not in a
public issue. Say what an attacker can do and how to reproduce it.

What matters most:

- Someone outside a team joining it, reading its messages or sessions, or sending into it.
- A team member speaking for another person or machine.
- Another local user or process talking to the daemon, or posing as it.
- A message making an agent do more than its user would see in the `<reason-message>` block.

Fixes ship in a new release, which the Reason CLI installs.

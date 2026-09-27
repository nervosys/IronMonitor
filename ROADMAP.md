# IronMonitor (ironmon) — Roadmap

- **Crate**: [`iron-monitor`](https://crates.io/crates/iron-monitor) 6.0.0
- **MSRV**: Rust 1.94, declared for every feature combination; a default build
  still compiles on 1.88 (see the unreleased section of
  [CHANGELOG.md](CHANGELOG.md))
- **License**: AGPL-3.0-or-later (commercial dual-license available)

This file used to list completed features, a release history and a platform
matrix. All three stopped at 1.x while the crate reached 6.0, so they are gone
rather than maintained twice. Each has a home that is kept current:

| For | Read |
| --- | --- |
| What shipped, and when | [CHANGELOG.md](CHANGELOG.md) |
| Features and platform support | [README.md](README.md) |
| The contract an agent relies on | [AGENTS.md](AGENTS.md) |
| Current state, and the detail behind every item below | [HANDOFF.md](HANDOFF.md) |

## The priority: readings that are true

The recent work has been less about new domains than about whether the existing
ones tell the truth. Every value carries a provenance — `measured`,
`specification`, `derived` or `unavailable` — and a value that could not be read
is reported as unavailable with a reason, never as a zero or a plausible
constant. A sweep of the codebase has found and fixed some thirty places that
did otherwise. The tests that keep them fixed compare each output path —
Prometheus, the MCP tools, the chat agent's context, the TUI and the GUI —
against the ontology.

New domains come after that, not instead of it.

## Next

### Needs hardware this project has not had

Each of these has code tested only against synthetic input, or only in its
absent form. One run on the right machine settles each.

- **A SATA drive** — the Windows ATA SMART parser has never read a real one.
- **A readable CPU temperature sensor** — `ironmon_cpu_temperature_celsius` has
  been checked only on a host that publishes no series.
- **A laptop** — `hardware_ai`'s form-factor weights were tuned on one desktop.
- **A Mac** — the Apple Silicon readers, including the ANE and CPU-cluster
  changes in the unreleased changelog, are cross-compiled but have not been
  run. macOS CPU and memory feed the ontology but not yet the pipeline the TUI
  and GUI draw from.
- **Bare-metal Linux** — RAPL's package power-limit path has never executed.

### Waiting on a decision

- Whether `fleet-store` ships in the next release. It is what raised the MSRV
  to 1.94, and `src/consent.rs` should be re-read against it if it does.
- What a DIMM's `capacity_bytes` and `populated` mean for an empty slot.
- Whether `PowerSnapshot`'s totals become `Option`, so "nothing measured" is
  expressible.

### Known and recorded, not yet fixed

- `ping` parsing assumes English output; on a localized Windows a reachable
  host reads as unreachable.
- macOS memory bandwidth comes from a brand-string table.

## Ideas carried from the 1.x roadmap

Not re-verified against the current code; treat each as a question, not a
status. Two items have shipped since the list was written and are removed:
automated tuning (`ironmon tune`) and local LLM support (`local-llamacpp`, via
the `llama-cli` executable).

- Live multi-host aggregation (gRPC/QUIC); alert routing (PagerDuty, Slack,
  email, webhook); SNMP traps; remote agent deployment
- FreeBSD; Intel Arc discrete GPUs; Thunderbolt devices; EDID parsing
- Natural-language system control; streaming responses in the GUI chat
- Custom dashboard layouts; system tray mode

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md), and its "Verifying a change" section
before opening a pull request. All contributions require signing the
[CLA](CLA.md).

## Notes

- Security-sensitive utilities in `src/utils/` require audit before production
  use.
- GPU control and other writes require elevated privileges, and go through the
  audited apply path described in [AGENTS.md](AGENTS.md).
- Datacenter features (IPMI) require `ipmitool` or sysfs access on Linux.

# IronMonitor (ironmon) — Roadmap

- **Crate**: [`ironmonitor`](https://crates.io/crates/ironmonitor) 7.0.7
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
- **An ARM64 Linux machine with 16 KiB or 64 KiB pages** — process memory now
  scales by the system page size; only the 4 KiB case has been run.

### Decided

- `fleet-store` ships in the next release, and the MSRV stays 1.94.
- A DIMM's presence comes from evidence, not from its size (done).
- `PowerSnapshot`'s totals become `Option` (done).
- The shared build directory moved to E: (a machine setting, not this repo).

### Known and recorded, not yet fixed

None at the moment. The four recorded here were fixed or, where a fix needed a
figure nobody could verify, turned into an honest refusal; HANDOFF has each.
Two ratchet tests keep the commonest defect shapes from growing back:
`tests/discarded_absence.rs` and `tests/partial_correction.rs`, whose baseline
of 76 untriaged structs is the largest body of remaining work.

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

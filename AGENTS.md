# Driving ironmon from an AI agent

ironmon's three interfaces — CLI, TUI and GUI — are built to be operated by a program,
not only by a person. This document is the contract.

The short version: fetch the schema, read values by id, check the provenance before
you believe a number.

```bash
ironmon describe --format json          # what exists: ids, units, provenance
ironmon describe --commands             # what can be run, generated from the parser
ironmon get gpu.0.thermal.temperature   # read one value
ironmon snapshot --validate             # read everything, range-checked
```

## Provenance is the part that matters

Every value carries where it came from. This is not bookkeeping — it is the
difference between a reading and a plausible-looking constant.

| Provenance | Meaning | Safe to treat as fact about now? |
|---|---|---|
| `measured` | Sampled from hardware or the OS this cycle | Yes |
| `specification` | A published constant — spec sheet, vendor table | True of the hardware, not observed here |
| `derived` | Computed from other entities, which it names | Only as far as its inputs are |
| `unavailable` | Not obtainable here, with a reason | No — render as unknown, never as zero |

Only `measured` satisfies `is_observation()`. Check it:

```bash
ironmon get gpu.0.power.limit --format json
{
  "id": "gpu.0.power.limit",
  "value": 450000,
  "provenance": "measured",
  "unit": "milliwatts"
}
```

An unavailable value carries no `value` field at all, and always says why:

```json
{
  "id": "gpu.2.clocks.graphics",
  "provenance": "unavailable",
  "note": "driver reports no graphics clock"
}
```

That distinction exists because this codebase repeatedly shipped invented numbers
through the same field as measured ones: a boot time of exactly 45 seconds assigned
whenever the real one could not be read, a GPU power percentage whose denominator
came from a core-count lookup table, Secure Boot reported from the existence of a
registry key rather than its value. Each was indistinguishable from a real reading
at the point of consumption. `provenance` makes the difference machine-checkable.

## Absence is reported, not implied

A domain that enumerates nothing still produces a row, so "this machine has no
disks" stays distinguishable from "IronMonitor does not read disks":

```
disk.<none>   unavailable   — no block devices enumerated on this machine
```

Truncation is announced for the same reason — absence from a capped list is not
absence from the machine:

```
process.<truncated>   unavailable   — 443 processes exist; this snapshot reports
                                      the 10 largest by memory
```

## Exit codes carry information

`ironmon get` distinguishes the two ways it can fail to give you a number:

| Code | Meaning |
|---|---|
| 0 | Read succeeded |
| 1 | No such entity id — check spelling with `ironmon describe --search` |
| 2 | Known id, no value available here |

Collapsing 1 and 2 would leave an agent retrying a typo forever, or abandoning a
device that is merely idle.

`ironmon snapshot --validate` exits 3 if any live reading is physically impossible.
It also withholds such values rather than clamping them: a clamped number looks
like a reading and is not one.

## Ids

Ids are dotted, stable, and shell-safe — no whitespace, so they need no quoting.
Device names chosen by vendors are sanitised into segments:

```
memory.total
gpu.0.thermal.temperature
network.Bluetooth_Network_Connection.rx_bytes
cpu.core.7.utilization
```

The schema uses templates (`gpu.{n}.name`) for anything with instances. A template
is a schema construct, not a question with an answer — `ironmon get gpu.{n}.name`
exits 1 by design. Expand it against a snapshot.

Ids may be added but never repurposed. `ironmon describe --format json` carries a
`version` so you can tell which contract you hold.

## Rates need two samples

A single query has one sample, so it cannot produce a rate. Counters are exposed
instead, and the rate fields say so rather than passing a counter off as a
throughput:

```
network.eth0.rx_bytes    measured      184320394 bytes
network.eth0.rx_rate     unavailable   — a rate needs two samples; this query took
                                         one — differentiate rx_bytes across two
                                         snapshots
```

## What can be changed

Reads are unrestricted; writes are not. `--writable` lists only entities backed by
a registered apply handler:

```bash
ironmon describe --writable
ironmon profile set active_scheme_guid <value> --confirm
```

Every attempt — allowed, refused or failed — is appended to the apply audit log.
Writing requires `--confirm`; the library never prompts and never elevates itself.

The write surface is generated from the handler registry, so the schema cannot
advertise a write the binary will reject.

`ironmon tune` obeys the same contract, including in its automatic mode. It detects
what the machine is being used for and recommends profile settings; `--watch N`
re-evaluates on an interval. It **writes nothing** unless given both `--apply` and
`--confirm`, and even then goes through the same audited path. Unattended
application is capped below the risk tier covering power, thermal, voltage and
MSR writes — `--max-risk dangerous` is rejected rather than clamped.

Every proposed value comes from what the driver declared: an entry in the
setting's own choice list, or its reported default. `basis` on each
recommendation says which. A setting whose provider enumerates no choices is
skipped with a reason rather than given a constructed value, so an agent reading
a plan can verify every number in it against the hardware that offered it.

## Reading the interactive surfaces

The TUI and GUI draw to a terminal and a window respectively, neither of which an
agent has. Both render headlessly instead.

```bash
ironmon tui --frame --tab CPU --width 160 --height 40
ironmon gui --frame --tab profiles
```

The GUI prints the text it actually painted rather than a screenshot, which
preserves a distinction pixels destroy: text that was never emitted looks different
from text emitted in an unreadable colour. That difference was a real bug — the
Profiles tab rendered all nineteen of its groups while every heading was drawn in
the panel colour, and looked dead.

An unknown tab lists the accepted names and exits 1, rather than silently handing
back a frame for a tab you did not ask for.

### Driving the TUI

TUI navigation is key-driven and stateful, so it can be scripted:

```bash
ironmon tui --script - <<'EOF'
goto CPU          # select a tab by name or index
key 5             # send a key through the real handler
assert Memory     # fail unless the frame contains this
refute Error      # fail if it does
capture           # print the current frame
refresh           # re-sample and take a fresh snapshot
EOF
```

Key steps go through the same function the interactive loop calls, so a passing
assertion is evidence about the TUI a person uses, not about a parallel
implementation of its bindings.

| Code | Meaning |
|---|---|
| 0 | Every step passed |
| 1 | An assertion failed — the message names the step and what was missing |
| 2 | The script did not parse |

### Inspecting the GUI

The GUI takes the same shape, minus `key`. Its tabs are addressable by name, so
`goto` covers navigation and there is no keystroke state to drive:

```bash
ironmon gui --script - <<'EOF'
goto profiles
assert Hardware Profile Inspector
refute Traceback
capture
EOF
```

A `key` step is rejected rather than ignored, and the error says why, so the
omission reads as a decision rather than an oversight.

Exit codes match the TUI's — 0 all passed, 1 an assertion failed, 2 the script did
not parse — so a caller can treat both surfaces the same way.

## Vocabulary is shared

The three surfaces agree on names. A domain is spelled one way everywhere, and a
label rendered on screen maps back to an id, so a user describing what they see can
be turned into something queryable. Tests enforce this rather than trusting
convention — including a scan of the GUI source that fails any heading spelling a
domain differently from the ontology.

## Guarantees worth relying on

- `ironmon describe` touches no hardware, so it is identical on every machine and can
  be fetched ahead of time and cached.
- Everything the resolver emits is declared in the schema, and vice versa.
- An unavailable reading never carries a value, and always carries a reason.
- No reader substitutes zero, a previous sample, or a plausible constant for a
  value it could not obtain.

These are asserted in `tests/agentic_contract.rs`, which drives the built binary
rather than the library — argv is the surface an agent touches, and a library test
would pass while `ironmon get` was broken.

## See Also

- [CLI.md](CLI.md) — full command reference
- [AI_INTEGRATION.md](AI_INTEGRATION.md) — connecting ironmon to a model provider
- [README.md](README.md) — project overview

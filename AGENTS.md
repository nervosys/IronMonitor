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

And a device that exists but could not be read is reported as that device, not
folded into "none". The other adapters are reported as usual, and indices do not
shift:

```
gpu.1.<unreadable>   unavailable   — this adapter was enumerated but its query
                                     failed: NVML: driver not loaded
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

## The HTTP surface

`ironmon serve` answers the same schema over HTTP, for a program that runs ironmon
next to it. Bodies are the CLI's own JSON: the same functions produce both, so the
CLI and the HTTP routes cannot disagree about a field.

```bash
ironmon serve --port 0 --api-key-file ~/.ironmon.key
# {"listening":"127.0.0.1:54321"}
```

The first line on stdout is that announcement, flushed before anything else. It is
the only thing on stdout: the banner and logs go to stderr. `--port 0` lets the OS
choose the port, and a taken port is a non-zero exit, never a silent move to another.

Every route is `GET`. Any other method gets 405 with `Allow: GET`.

| Route | Body | Status |
|---|---|---|
| `GET /api/v1/describe` | Exactly `ironmon describe --format json`, with a trailing newline. Touches no hardware and starts no sampling. | 200 |
| `GET /api/v1/snapshot` | The envelope of `ironmon snapshot --format json`: `ontology_version`, `resolved`, `unavailable`, `impossible`, `readings`. Each reading object is the CLI's, plus `sampled_at` and `age_ms`. | 200, 400 for an unknown `?domain=` (body lists `accepted`), 503 while sampling |
| `GET /api/v1/get/<id>` | The CLI's reading object for `<id>`, plus `sampled_at` and `age_ms`. The id is percent-decoded, so `disk.%3Cnone%3E` is `disk.<none>`. | 200 for a value and for `provenance: unavailable` (no `value` field, a `note`); 404 for an undeclared id, a template, or an id this machine did not produce; 503 while sampling |
| `GET /api/v1/health` | `status` and `timestamp`, plus `sampler`: `state` (`running` or `idle`) and per-segment `last_sampled_at` and `age_ms`. | 200 |

A 404 is `{"error":"no such entity id","id":...}`. A 503 is:

```json
{"state":"sampling","segments":["gpu","inference"]}
```

with `Retry-After: 1`. It means nothing has been learned yet for those segments. It
is not an `unavailable` reading, and the client should retry.

### Sample age

Every reading from these routes says when it was sampled. `sampled_at` is RFC 3339 in
UTC, stamped with the moment the pass began, and `age_ms` is the milliseconds since
then, measured when the response is built. A reading never appears fresher than it
was. Segments are sampled at different times, so age is per reading, not per response.

### Sampling

The server samples by segment (the ontology domain), using the same readers and the
same segment closure that `ironmon get` uses. A request marks the segments it needs as
wanted, then waits at most 250 ms for their samples. Cheap segments answer on the first
request. Expensive ones answer 503, and a later request gets the sample.

One background thread runs every pass, one at a time, because the readers share
static state. Concurrent requests for a segment share one pass. A wanted segment is
resampled as soon as its last sample is two seconds old, so a slow segment lags and
its age shows the lag.

A segment that nothing has asked for in 60 seconds is no longer sampled. When no
segment is wanted, the sampler thread blocks with no timer and runs no pass, so a
server left running with nothing reading it does no sampler work and starts no
subprocesses.

Rates are not differenced. A rate needs two samples, and the HTTP routes keep the
CLI's single-sample behaviour: a rate field is `unavailable` with the CLI's reason.
Differentiate the counters across two responses instead.

### Keys and access

`--api-key-file <path>` or `IRONMON_API_KEY` sets a bearer key. Every route except
`/api/v1/health` then requires `Authorization: Bearer <key>`. The comparison is
constant time, and a missing or wrong key gets 401 with no detail. Without a key set,
the routes are open, which is acceptable only for loopback.

`/api/v1/health` without a valid key returns liveness only, with no sampler state. An
empty key, an unreadable key file, or both sources at once is a startup error. The key
is not a command-line value and never appears in output, logs or `Debug` formatting.

Prometheus can send the same header. In `prometheus.yml`, use
`authorization: { credentials_file: /path/to/key }` on the scrape job.

Responses carry no CORS headers unless `--cors-origin` names the requesting origin
exactly. `*` is refused at startup, because it would let any web page read the
telemetry.

When bound to loopback, a request whose `Host` header is not `localhost`,
`127.0.0.1` or `[::1]` (with or without a port) gets 403. A web page can point a name
at 127.0.0.1 and read responses as same-origin, which CORS does not stop, so the Host
header is the check that does.

A non-loopback `--bind` without a key starts with a loud warning. That setup is
documented in `grafana/README.md`, so it is allowed rather than refused.

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

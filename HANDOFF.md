# Status

Current as of 6.0.0. This file is excluded from the published crate
(`Cargo.toml`'s `exclude` list) and is for whoever picks the work up next.

## Read this first: 6.0.0 is shipped

Merged to `master` as a fast-forward and tagged **`v6.0.0`** at `1947426`, with
**CI green on all three platforms**. Not published to crates.io — declined for
5.2.0 and 6.0.0, tag only.

**`master` has moved on since the tag, and CI is green again as of `e629af8`** —
all seven jobs, Windows included, for the first time since 2026-09-02. It had
been red for five days on two separate faults, one per platform, and neither was
visible from a local run on this machine. Both are described below under *The
line that made every Linux job red* and *The crash with no panic*.

| Commit | What |
|---|---|
| `9452a01` | Eleven clusters, five honesty defects, five nullability fixes |
| `2e6f9e9` | The `CACHE_STATS` test race |
| `b2dd07b` | `gpu.codec` declared `Measured` for rows resolving `Specification` |
| `da769bf` | A clippy 1.98 lint that would have failed CI |
| `b7bc21d` | Two more non-nullable entities a VM resolves absent |
| `099c604` | The provenance guardrail: stronger than declared is fine, weaker is not |
| `2687dbb` | `memory.bandwidth` published defaults; `services` became a summary |
| `ebab956` | Nine readers said "none" where they meant "cannot look" |
| `1947426` | MSRV raised to 1.89, true of every feature combination |
| `014e4dd` | Kernel parameters, split from this crate's opinions about them |
| `24a7314` | The macOS CPU path in the CLI backend invented almost everything |
| `7b606fe` | A GPU-holding Python process was reported as PyTorch |
| `1cf1980` | The agent-facing memory tool invented figures on macOS |

Since the tag, on `master` and green on all three platforms:

| Commit | What |
|---|---|
| `afb377d` | Secure Boot was readable all along, unelevated |
| `2f7be76` | Boot duration bound; 32 descriptions printed collapsed indentation |
| `8961a1c` | The agent surface reported an uptime of zero and a load average |
| `4dd5cce` | A test for the agent tool surface, which had none |
| `a584dd0` | Five wrong conclusions in `hardware_ai`, each with a confidence attached |
| `7607401` | A 24GB card read as 4GB, because `AdapterRAM` is 32 bits |
| `00ffef8` | The method, written down, and what the one-machine caveat costs |
| `6617020` | The nominal clock reported as the current one, on every core |
| `ecd1281` | Two more constants wearing a measurement's provenance |
| `7192187` | Four NVMe SMART fields read sixteen bytes early |
| `4846c3f` | An interlaced mode reported field lines as a resolution |
| `1e26d01` | The SATA temperature walk used the input struct's layout |
| `4af44bc` | The three scans, in the order they pay |
| `7c79f8b` | A DIMM at zero volts, and three absence words as values |
| `3a50c99` | `pstate_name` gated with its caller, after CI caught it |
| `2360068` | Watching nothing is not the same as having no baseline |
| `5931069` | The recorder stored an absent sensor as zero degrees |
| `1666a52` | A shell variable read as a debugger, and a consent UI for nothing |
| `f9e7248` | `serve --help` named a Prometheus route that 404s |
| `425ff4a` | 24 threads reported as 24 cores, one line under a 12-core name |
| `1284391` | `cli temperature` found no sensors while snapshot read five |
| `dcbd50b` | A display's mode had nowhere to be absent; five consumers printed 0x0 |
| `c14e8c6` | A gamepad counted as a Bluetooth radio, GATT services as devices |
| `9ef5ba0` | The PnP classifier gated with the only target that calls it |
| `cc7aac6` | Root hubs given the identifier 0000:0000, and an invented Intel hub |
| `b16d4fb` | A terminal and a browser classified as GPU compute |
| `19a6a06` | Per-process GPU memory: NVML says N/A, simon said 0.0 MB |
| `1a6718c` | Master volume 100% on every machine, and setters that changed nothing |
| `308c770` | The invented-device rule generalised past displays |
| `0121d86` | A clean bill of health drawn from 2 of 23,541 settings |
| `f34f72c` | "No engines detected" on a platform with no engine reader |
| `916bf08` | A sentinel introduced by an earlier fix in this same session |
| `49e40ef` | The same nominal-clock lie, in a second reader of the same API |
| `eaf7e3f` | A fifth AdapterRAM reader, on the public API, still capped at 4GB |
| `9bc2524` | NPU core counts guessed from a vendor guessed from a name |
| `b2e84af` | A doc comment that named the danger, ignored by the caller two lines away |
| `8f8680a` | One system-wide number, copied into 24 cores as though each were measured |
| `b16feec` | A link's capacity published as the traffic on it |
| `1c018b8` | A typical NVLink count reported as this board's |
| `a015d58` | Every AMD CPU classified as Intel, by the word in "12-Core" |
| `ad8dfff` | A CPUID family of 0, and a name still carrying its WMI padding |
| `3935301` | `is_turbo` closed, and the guess above its known defect |
| `dd2f56a` | Secure Boot read from a file's existence, not its value |
| `05468fb` | An unread thermal zone reported 0 C and "not throttling" |
| `0e67be4` | A failed health check reporting no critical issues |
| `aed27cd` | A `> 0` guard defeated by a sentinel of 64 |
| `f5a54ee` | An `Option` field filled with a sentinel wrapped in `Some` |
| `cd15e37` | Cache and block geometry assumed rather than read |
| `ae625ab` | A rate published as a total, for the wrong drive |
| `89b6cd9` | A failed enumeration reported as an empty machine |
| `acf4e69` | The same swallow in sixteen more enumerators |
| `57c7eb9` | Two enumerations, both failing, reported as one empty |
| `3485fbb` | "This machine has no TPM", published as a measurement |
| `a053f16` | Measured boot reported off, on a host where it is on |
| `e913250` | SIP read as Secure Boot, and advice from an unread flag |
| `f1470d5` | Fifteen codecs, one frame rate: a constant with a derivation |
| `599e8ff` | An idle webcam reporting that it was streaming |
| `7fde63e` | A green gate that could not see the defect it shipped |
| `0106d6c` | The same field, right on two platforms and invented on the third |
| `bde8726` | Sockets counted as NUMA nodes, memory divided evenly between them |
| `3606758` | A field derived from the one thing it exists not to be derived from |
| `e037198` | The same inequality again, this time against a string |
| `ca71102` | "On battery" concluded from nobody having asked |
| `c79d4c5` | Nine tests asserting a contract the readers no longer make |
| `6fbf520` | A device's name read as the speed it negotiated |
| `6f289bd` | Three displays where one exists, two of them graphics cards |
| `4b004c7` | Twelve audio endpoints where four exist, two facing backwards |
| `4410ea6` | Every NVMe device counted twice, once as its own controller |
| `9a9b585` | A USB stick with no SMART, passing its SMART check |
| `9332a38` | An id documented to be stable, built from enumeration order |
| `c3391f0` | The health reader's own enumeration could not report a failure |
| `4ed9145` | Boot mode asserted because it is usually right |
| `15a60ab` | 97 GB of swap in use, on a machine using 3.4 GB of it |
| `5bd14aa` | The last six enumerators, and what the list was really for |
| `7fcddde` | The last swallow, a real USB key, and a swap series that changed meaning |
| `2276c9c` | Per-core clocks, from the counter that was named three fixes ago |
| `3197ee0` | A flat battery that was never read, and the test that caught me |
| `066e373` | An invariant the ontology states and this machine breaks |
| `cab0c34` | Auditing every absolute claim in the ontology against the machine |
| `e456c53` | A class guessed from the device's name, when it declares one |
| `44612d8` | The last misleading absence, and a deliberate refusal to implement |
| `68c9ff0` | Every measurement, checked for whether it actually moves |
| `8ed7e71` | Zero hertz and zero swap, handed to an agent as facts |
| `e425908` | Every network rate in the crate was zero, on every platform |
| `17dd4fa` | Two fabricated zeros next to the comment explaining why zero is wrong |
| `25bcca8` | A 16 MB drive reported as sizeless, by integer division |
| `9686675` | A regression I introduced, found by the sweep that follows the fix |
| `cc4e347` | The zero left behind when a field stopped being the identity |
| `4685c52` | A guard for the regression, and nearly a test that proved nothing |
| `db94a7b` | The two pairings the guard did not cover, and why one is separate |
| `a3c8415` | A metrics endpoint Prometheus would reject in full |
| `74b635d` | Dashboards querying names nothing published, and three wrong tests |
| `d4da86c` | Two Prometheus renderers, and the server serves the worse one |
| `5796371` | The instance in the metric name, and a rate called a total |
| `15ea71e` | Doing the thing the previous commit deferred |
| `a50b77d` | The sentinel put back at the point of use |
| `d4c4281` | Two auth flags that decide nothing, and the probe that found them |
| `69f1766` | The last unaudited surface, and a fix that never reached the screen |
| `fc1667e` | A contract that was right for a loop and wrong for everything else |
| `86fc72c` | Checking my own open-work item, which was already stale |
| `f64f5ea` | Disk throughput, zero on every platform, drawn on three surfaces |
| `ed4ec44` | A section of the state document that was never filled in |
| `5a8bcee` | 26 public types the crate declares and never produces |
| `3656368` | Six uncalled readers that answered zero for unknown |
| `495e2ba` | Two renderers covering for each other, and a test that vouched for itself |
| `f194cfd` | A rate published as a counter, and 530 processes reported as 0 |
| `2e33bdf` | A load average synthesised from one CPU reading |
| `3ccaf43` | The four dashboard metrics the exporter never learned |
| `400608c` | One WMI connection instead of N, and the counters that cost |
| `cd07a27` | The last dashboard gap, and an empty gap list on both sides |
| `2c80d66` | The negotiated USB speed, and nothing at super speed |
| `0801f11` | Six recorded columns that wrote zero for a failed read |
| `556a471` | A type that could not say "not reported", and four backends that filled the gap with zeros |
| `549ca3e` | GPU utilization, and an unreadable card advertised as idle |
| `76dc998` | Six power readings unwrapped to zero, and "No swap configured" |
| `162825a` | The backend and CLI swap sites that could not say "not read" |
| `3073bf3` | The rest of the swap surfaces, and a health check that vanished |
| `7d78dab` | A DIMM voltage the query never asked for, and MT/s called "count" |
| `b76c215` | An absence reason contradicted by the rows beside it |
| `dbe64e0` | A manufacturer the query never selected, and Windows' fake vendors |
| `91667d1` | The CPUID triple Windows reports, under a comment saying it does not |
| `235562e` | The cache topology this module's docs already claimed to read |
| `e6d5259` | The transport a HID node's parent names |
| `59e6268` | A rated figure published as measured, on 24 rows |
| `64e5cf3` | The mixer binding the absence reasons said did not exist |
| `ada6755` | Four absence reasons that blamed the platform for a column nobody selected |
| `b329677` | Three absences the platform had answered, and six reasons that blamed a driver nobody asked |
| `beaef09` | The settings the binary could already read, and three PCI addresses it had |
| `8fe9da9` | A source for every SMART absence, and the RNG the feature list already named |
| `f4ea43d` | The current year from the clock, and the power cap from the driver |
| `001f764` | The line that made every Linux job red since 2026-09-03 |
| `e629af8` | The crash with no panic: COM interfaces released after their apartment |
| `448a289` | A two-GPU machine graded on one GPU's power budget |
| `9df8e67` | The CPU's own power limit, where the platform states one |
| `367429a` | 93.6 GiB of RAM published as 98 MB, and the test that found it |
| `7abbd92` | The agent tool surface's numbers, against the ontology's |
| `282fbaf` | The JSON-LD manifest had no tools in it |

**None of these were found by grepping.** The method, and why the greps missed
them, is below under *Run it and read the output*.

### Two renderers, one machine, and a factor of 1024

The open-work list had run out of things this machine could settle, so this is a
new check rather than an old item: **`PrometheusExporter` and
`ontology::resolve::snapshot()` are two independent collection paths over the
same hardware, and nothing had ever compared their values.**
`prometheus_exposition.rs` checks names, headers, label syntax and dashboard
coverage. All of it passes while a renderer reports kilobytes under a name
ending in `_bytes`.

It did:

```
simon_memory_total_bytes = 98191140      memory.total = 100547727360
```

Exactly 1024x. `RamInfo` is in KB — its own doc comment says so on every field —
and the exporter published `mem.ram.total` unscaled, so a 93.6 GiB machine
reported 98 MB of RAM. **The swap block three lines below has always scaled**,
with the comment "`SwapInfo` is in KB; the metric name says bytes" sitting right
there. Somebody saw it for swap and not for the three fields above it.

The served endpoint in `http_server.rs` renders from `pipeline::Snapshot`, a
different type in the correct unit, and was right all along — so the two
renderers had disagreed by three orders of magnitude for as long as both have
existed, and **each looked correct when read on its own**. That is what a
cross-surface check buys: neither surface is wrong in a way it can detect.

**`tests/prometheus_agreement.rs` compares only what cannot move.** Installed
memory, swap size, an adapter's total VRAM. Everything else differs between two
reads for reasons that are not defects — this machine reported 80.8% CPU through
the endpoint and 22.4% through the ontology in the same minute, and both were
right. A test that compared everything would fail constantly and be deleted; one
that compares the stable half fails only when something is wrong. It also
asserts that it compared *something*, because a surface that stops publishing
totals would otherwise make it pass by having nothing left to check.

**Where to point this next.** The same technique applies to every pair of
surfaces this crate has over one machine, and it had found two defects in two
attempts when this was written: the active power scheme (`simon profile explain`
versus a "no resolver bound" absence) and the exporter above.

**The third attempt came back clean**, and that is worth as much in a file like
this: `tests/agent_agreement.rs` (`7abbd92`) compares `AiDataApi::call_tool` --
what an LLM reads through MCP -- against the ontology, and installed memory,
swap size and the logical core count all match exactly. Nobody needs to wonder
about that pair again.

Its first draft, though, **passed while comparing almost nothing**. It looked for
`cores.physical` in `get_cpu_status`, which reports `core_count`; the pair
silently skipped and the test went green on the memory figures alone. A missing
path is a failure there now, with a message saying the field was renamed or
dropped. **In an agreement test the dangerous outcome is not a wrong comparison
but an absent one**, because both look identical from the outside — green.

Verified by mutation, which is the cheapest possible way to find out whether an
agreement test can fail at all: pointing `core_count` at `cpu.cores.physical`
turns it red with `24 (x1 = 24) but cpu.cores.physical = 12`.

**The fourth attempt found the largest omission yet, and not by comparing
numbers.** `simon ai manifest --format jsonld` returns 295 bytes; every other
format returns 21-25 KB. `to_json_ld` ignored `self.tools` and emitted six fixed
keys naming the application — a manifest of nothing, offered in the CLI's help
beside the nine formats that answer. **The CLI exits zero and the JSON parses**,
which is why a surface can stay empty for as long as this one did: it is not a
failure, it is a valid answer to a question nobody asked.

`every_format_carries_every_tool` (`282fbaf`) counts tool names in each format's
serialised output rather than knowing any format's shape, so it holds for a
format added by someone who never reads this file. Without the fix: `JsonLd omits
52 of 52 tools, starting with get_system_summary`.

**Four pairs checked, three defects, and they were not the same kind.** A unit
error (KB under a `_bytes` name), a reader that existed on another surface (the
active power scheme), and an exporter that dropped its payload entirely. The only
thing they have in common is that each surface looked correct when read on its
own — which is the argument for checking surfaces *against each other* rather
than more carefully one at a time.

**The fifth and last pair is `simon cli`, and its human output is right.**
`simon cli memory` prints `62.68 / 93.64 GB`, which is the ontology's
100,547,727,360 bytes to the digit. The renderer converts correctly; nothing to
fix.

**Its JSON mode is a different matter, and it is queued rather than changed.**
`simon cli memory --format json` serialises the library struct as it stands, so
it emits `"total": 98191140` — kilobytes, under a key that does not say so, on a
machine whose other three surfaces all report bytes:

| Surface | `memory.total` |
| --- | --- |
| `simon snapshot` | 100547727360 (entity declares bytes) |
| `/api/v1/metrics/prometheus` | 100547727360 (name says bytes) |
| `simon cli memory` (human) | 93.64 GB |
| `simon cli memory --format json` | 98191140 (kilobytes, unmarked) |

Nothing here is *wrong* — `RamInfo` documents KB on every field — but the number
cannot be interpreted from the output alone, and this is the third convention
across one set of surfaces. **That is the same fault line the exporter fell over
in `367429a`**, which published the same field unscaled under a `_bytes` name.
Changing the JSON's units or adding a unit marker is a change to a public output
format, so it is item 7 of the 7.0.0 list rather than a fix made at the end of a
session.

### The other half of the envelope

Item 4's last open limit, and the answer is the same shape as the GPU's: the
platform states a power limit, and the crate was guessing one from a model name.
RAPL's `constraint_0_power_limit_uw` is the long-term package limit -- the
configured TDP -- and `rapl::RaplMonitor` already reads and parses it. The
envelope takes it where it exists, sums the package domains so a two-socket
machine is priced as two, and `ThermalEnvelope::cpu_tdp_measured` says which of
the two figures a consumer holds.

That flag is the point. The envelope's total mixes a **measured** 900 W GPU
budget with a **guessed** 120 W CPU one, and nothing distinguished them:

```
CPU TDP:  120 W (the model-name table; no package limit was readable)
GPU TDP:  900 W over 2 adapter(s) (the caps the drivers enforce)
total:    1050 W
```

**Half of this is unverified, and it is the half that reads.** `RaplMonitor::new()`
fails on Windows -- the MSRs need a kernel driver, there is no sysfs equivalent --
so this machine exercises the `Err` path, the table, and the flag reading false.
No Linux machine was available. The parse it depends on is covered by `rapl`'s
own tests; the wiring is not. **`measured_cpu_power_cap_watts` says that in its
own doc comment**, which is the difference between a known gap and a surprise:
item 18 of this file exists because a reader written by inspection and not
labelled as such is indistinguishable from one that has run.

### The second card

Reading a real power cap instead of a table guess had a tail: the reader took the
**first** adapter that reported one, and this desktop has two RTX 3090 Ti. The
envelope priced 450 W of a 900 W GPU budget and graded a 1050 W machine as a
600 W one.

```
GPU TDP:  900 W over 2 adapter(s) (the caps the drivers enforce)
total:    1050 W
```

`ThermalEnvelope::gpu_adapters_counted` is new, and it is the point of the
commit as much as the sum is: **"450 W" and "900 W over two adapters" are
different claims and the number alone cannot tell them apart.** The same
distinction as `gpu_tdp_measured` beside it, one level up.

The AMD integrated GPU in the same machine reports no cap, so it contributes
nothing and is not counted — not priced at zero, not guessed at. Where *no*
adapter reports a cap the name table still answers and the count is 1, because
the table is keyed on a single model string; multiplying it by an adapter count
would be compounding a guess with a fact.

**Worth noticing about the shape of this session:** the defect was created by the
fix two commits earlier, and it was recorded in the handoff at the time — item 4
said "the envelope counts one GPU on a machine with two" — before anyone fixed
it. Writing the limitation down is what made it cheap to close later. **A known
limitation in a file someone reads is worth more than a fix nobody remembers
making.**

### The crash with no panic

Pushing the Linux fix turned four jobs green and left one red: **Test
(windows-latest)**, dying with

```
error: test failed, to rerun pass `--lib`
note: test exited abnormally; to see the full output pass --no-capture to the harness.
```

No panic, no assertion, no failing test named — the lib test binary simply gone,
61 tests in. The same signature is in the previous run, four days earlier, so it
was not something this session introduced; the last fully green run was
2026-09-02, and **the Windows fault and the Linux one arrived in the same batch
of commits.**

The 61 completed tests are alphabetically early, and the last one printed is
`audio::tests::test_audio_device_serialization`. The three audio tests that
construct an `AudioMonitor` were the ones still running. `64e5cf3` — the mixer
binding, landed 2026-09-03 — is in the batch.

```rust
let initialised = CoInitializeEx(...).is_ok();
let enumerator: IMMDeviceEnumerator = CoCreateInstance(...);
...
if initialised { CoUninitialize(); }
// <- `enumerator` dropped HERE, after the apartment is gone
```

`IMMDeviceEnumerator` is reference-counted and its `Drop` calls `Release`. Rust
drops it at the end of the scope it was *declared* in — the same scope as the
`CoUninitialize()`, and therefore after it. **Releasing a COM interface into a
torn-down apartment is undefined, and on Windows it is an access violation: a
process that dies without a panic, which is the only thing the harness can
report.** Every interface now lives in a labelled `'com` block that closes before
the uninitialise.

**Why 896 local tests never saw it.** COM is per-thread and first-in wins. On
this machine the `wmi` crate has usually already initialised COM on the thread
that runs these tests, so `CoInitializeEx` finds the apartment up, `initialised`
is false, and `CoUninitialize` never runs — the bug is real on every machine and
*fires* only where this reader is first in, which on a fresh runner thread it is.

**Three lessons, and the third is the one worth keeping.**

- A crash is not a test failure. `--lib` with no test name and "exited
  abnormally" means a signal or an access violation, and no amount of reading
  assertions will find it. Look for undefined behaviour, and look at what was
  running rather than what failed.
- Alphabetical position is evidence. The harness starts tests in order, so the
  last few printed bound the region where the process died.
- **A latent UB bug is a coin the environment flips.** This one was written
  correctly enough to pass review, pass 896 local tests, and destroy a CI runner
  — the difference being whether some *other* library got to COM first. The
  local suite could not have caught it, and no amount of running it more times
  would have.

### The line that made every Linux job red

**`gh run list` was the last thing on the verification list and the first thing
that should have been run.** Master's most recent CI run — `ae2cd74`, the handoff
commit this session started from — has been **red since 2026-09-03**: Check,
Clippy, Test (ubuntu) and Feature combinations all failing, macOS green, and
twelve local commits stacked on top of it before anybody looked.

One line, in `src/platform/linux/gpu.rs`:

```rust
let load = utilization.map(|u| u.gpu as f32).unwrap_or(0.0);
```

`GpuStatus::load` became `Option<f32>` in `549ca3e` — the fix that stopped a card
at 0% and a card whose counter could not be read being the same number. This path
kept its `unwrap_or(0.0)`, so it kept publishing the second as the first, and
because the field is now an `Option` it also stopped compiling: `error[E0308]`,
one file, every Linux job.

**Three independent things had to line up for it to survive four days:**

- The block is `#[cfg(feature = "nvml")]`, so a Windows compiler never reads it.
- `nvml = ["nvidia"]` is a *separate* feature from `nvidia`, and the handoff's
  cross-check feature list named `nvidia` and not `nvml`. **The documented local
  Linux check compiled the file's neighbours and not the file.**
- The last green run was two days earlier, and nothing between them ran
  `gh run list`.

The fix is `let load = device.utilization_rates().ok().map(|u| u.gpu as f32);` —
the same shape `549ca3e` applied everywhere else, and the compile error and the
honesty defect had exactly one cause. The cross-check command in *Verification
that is worth repeating* now carries `nvml`, and the Jetson-shaped
`gpu_type: GpuType::Integrated, // Could be discrete` two lines below it now says
`Discrete`, which is what an NVML-enumerated card on a desktop is.

**The lesson is the one already written three paragraphs above it, and it still
had to be learnt again.** "Read CI before believing a green local run" is in this
file, in bold, with the last occurrence spelled out — and a session that ran
`cargo fmt`, `cargo clippy -D warnings`, the MSRV check, both cross-target checks
and fourteen integration suites still did not run the one command that would have
shown a red pipeline in two seconds. **Local verification tells you your changes
are fine. It cannot tell you the branch was already broken.**

### A hardcoded year, and a guess beside a measurement

The absence audit has run out of things this machine can decide, so this is item
4: `infer_gpu_year` and the TDP tables, which the last handoff recorded as
unaudited. Three things were wrong, and one of them was wrong about the calendar.

**`let current = 2025.0;`** — the year every age in the report is measured
against, written into the source in three places. It is not a stale constant
that will be noticed; it is one that ages a year every January while continuing
to produce plausible numbers. On this machine, in 2026, every age was a year
short. **A hardcoded release year is a fact about a product. A hardcoded current
year is a fact about whoever last edited the function.**

The test for it asserts the property rather than the value — that
`current_year()` tracks the clock — because a test that asserted `2026.7` would
be the same defect with a green tick on it.

**The 3090 Ti dated from the RTX 30 series**, which the handoff already knew:
2020 by the `rtx 30` rule, 2022 in fact. Two years out for the one card this
table has ever been checked against. Refreshes with known dates are matched
before the series rule now, and no further: a rule per card would be a release
database, and this is a heuristic that reports 0.85 confidence — which is still
high for a substring table, and still item 4.

**And the GPU TDP was a guess sitting beside a measurement.** `infer_gpu_tdp`
returns 350 W for anything matching "3090". This machine's 3090 Ti enforces a
450 W cap, and NVML reports it to the same process through this crate's own GPU
monitor — the fourth appearance of "the reader was in the same binary", and the
first where the wrong number propagated: `analyze_thermal_envelope` sums the TDPs
and grades cooling from the total.

`ThermalEnvelope::gpu_tdp_measured` now says which of the two a consumer holds.
**A measured 450 W and a guessed 350 W are different claims, and they were
reported identically** — the same distinction `Measured` and `Derived` draw in
the ontology, which the analysis surface did not have.

**The report on this machine, which nobody had ever printed:**

```
class:            Workstation (confidence 0.60)
performance tier: Ultra (score 83)
CPU year:   Some(2024)      GPU year:   Some(2022)
age:        3.7 years (confidence 0.85)
CPU TDP:    120 W (from the model-name table)
GPU TDP:    450 W (the cap the driver enforces)
total:      600 W   headroom: Marginal   cooling score: 40
```

Before the fixes: GPU year 2020, age 2.5 years, GPU TDP 350 W, total 500 W.

**`examples/hardware_inference.rs` is new because the handoff asked for something
that could not be run.** Item 4 says to run `full_analysis()` on a real laptop
and read the report; there was no surface that did, so doing what the handoff
asked meant writing this file first. It exists now: one command, and the report
above is what it prints.

**Two things the report shows that are still wrong, and are not fixed here:**

- ~~**The envelope counts one GPU.**~~ **Fixed in `448a289`**: the caps of every
  adapter that reports one are summed, and `ThermalEnvelope::gpu_adapters_counted`
  says how many did. This machine reads 900 W over two adapters, 1050 W total.
- ~~**`infer_cpu_tdp` is still a table**~~ **Addressed in `9df8e67`**, as far as
  this machine can: RAPL's package power limit is preferred where the platform
  states one, and `ThermalEnvelope::cpu_tdp_measured` says whether it did.
  Windows states none, so the table still answers here — `ryzen 9` returns 120 W
  for a whole family, right for this 9900X and wrong for a 9950X (170 W) — and
  the flag reads false. **The reading half has never run**: it needs a Linux
  machine, and that is what is left of item 4.

### Which source said nothing

Two more reasons checked, both false, and both the same mistake in a different
module: **the reader attributing its own silence to the hardware.**

**Four SMART counters blamed a drive nobody asked.** `disk.0` is the USB
mass-storage gadget, and its counters come from `Get-StorageReliabilityCounter`
— which on this machine fails for *every* disk unelevated:

```
0 [NVMe]: FAILED - Access to a CIM resource was not available to the client.
3 [USB]:  FAILED - Access to a CIM resource was not available to the client.
```

The rows said "the drive did not report a power cycle count", "…a power-on hour
count", "…an uncorrectable count", and — the one that is a claim about hardware —
"this device exposes no thermal sensor".

`SmartInfo` is four `Option`s and no record of which of three paths filled them,
so the resolver had one sentence for every `None`. It carries `SmartSource` now:

| Source | What a `None` means there |
| --- | --- |
| `NvmeLogPage` | The drive answered from its own log page. A missing sector counter is a fact about NVMe, which has none |
| `AtaAttributes` | The drive returned its own attribute table. A missing counter is a fact about *this drive's* table |
| `StorageStack` | The operating system was asked, not the drive. An absence is usually the query being refused |

The same field fixes a disjunction that was wrong on all four disks:
`"not an NVMe concept; on ATA, the drive did not report attribute 5"` names two
situations and says which applies to neither. **A reason that covers every case
explains none of them** — and it is the tempting shape to write when the type
in front of you cannot tell them apart. The fix was to make the type able to say.

**And a random source the same report already named.** `detect_rng` read
`/dev/hwrng` and `/sys/class/misc/hw_random`, on Linux only, so on Windows
`cpu.crypto.rng.<none>` said "the probe enumerated nothing, but the feature list
above reports a random number instruction — this is a gap in the probe on this
platform rather than a machine without an RNG". In the same snapshot:

```
cpu.crypto.feature.2.name = RDRAND
cpu.crypto.feature.3.name = RDSEED
cpu.crypto.rng.<none>     = the probe enumerated nothing
```

The features come from CPUID and are detected two lines above the probe.
`RngSource::CpuInstruction` was declared for exactly this case and produced by
nothing. Passing the features in closes it: two sources, `quality` absent because
neither declares an entropy figure anywhere readable.

**This one raises the absence count**, from 437 to 438, and that is the right
direction: one diagnostic saying the machine had no random source is replaced by
six readings and two honest per-source absences. *Fewer absences is not the
goal.*

### The reader was in the same binary

Three more reasons checked, all three false, five absences closed. One of them
is not a question about Windows at all.

**`cpu.setting.active_scheme_guid` was published as "no resolver bound on this
build — the entity is defined but simon does not yet read it here".** In the
same binary:

```
$ simon profile explain active_scheme_guid
  value:     381b4222-f694-41f0-9685-ff5bb260df2e
```

`EntityKind::Setting` entities are generated from the apply-handler registry —
that was itself a fix, so the ontology could not promise a write no handler
backs — and nothing ever read one *back*. Every setting fell into the sweep that
reports an entity nothing produced. So the ontology said the machine could not be
read while another surface over the same machine was reading it, using a
`read_current()` that item 18 of this file describes and that
`ApplyHandler` already calls on every write to record what it overwrote.

`resolve_settings` now publishes it, through the same retry that keeps a
transient failure from being mistaken for an unreadable setting. A handler with
no `read_current` is one-way by construction, and the absence says so instead of
blaming the build. **The lesson is narrower than "check the reasons": an absence
reason is a claim about *simon*, too, and this crate has more than one surface
over the same hardware.**

**Three PCI addresses that were in the registry all along.**
`Win32_SCSIController.DeviceID` is a device instance path, not an address, and
the ontology's `looks_like_pci_address` guard correctly refused to publish it —
under a reason claiming the value "cannot be joined against `pci.*`". It can:

| Controller instance path | `LocationInformation` | Node in the same snapshot |
| --- | --- | --- |
| `PCI\VEN_144D&DEV_A810&…` | PCI bus 2, device 0, function 0 | `pci.0000_02_00_0` |
| `PCI\VEN_144D&DEV_A808&…` | PCI bus 13, device 0, function 0 | `pci.0000_0d_00_0` |
| `PCI\VEN_144D&DEV_A80C&…` | PCI bus 6, device 0, function 0 | `pci.0000_06_00_0` |

The lookup was already written, in `pci_devices`, for the PCI enumeration's own
use. **A guard that refuses a bad value is not the same as a reader that finds
the good one**, and the reason attached to the refusal is where the difference
gets recorded.

The `ROOT\SPACEPORT\0000` and `SWD\XVDDENUM\…` controllers keep their instance
path and their absence, because that reason is the informative answer: they are
not on a PCI bus.

**And a TCP setting hidden behind `Select-Object -First 1`.**
`Get-NetTCPSetting` returns one row per *template*, not one row for the machine,
and the first is `Automatic` — the selector, which carries no level of its own
and returns an empty line. The other six all say `Normal`. So
`system.kernel_param.<none>` reported "no kernel parameter was readable here"
about a setting the machine states six times, and the whole kernel-parameter
domain was empty on Windows because of a `-First 1`.

`agreed_tcp_level` reads every row that states a level and publishes the value
only when they agree. Templates that disagree describe different destinations
rather than one machine-wide level, and picking one of them would answer a
question nobody asked.

**Two `-First 1` defects in two passes** — this one and the display connection in
`b329677`, where the first of two WMI nodes for one panel said
`D3DKMDT_VOT_UNINITIALIZED` and the second said HDMI. Worth a grep: taking the
first row of a list the platform did not promise was ordered is a shape, not a
coincidence.

### A guard that caught the handoff, and ten more reasons

Fourteen more absence reasons checked, ten false. First, though, the thing that
found itself: **`every_documented_command_exists` failed on the previous commit's
own handoff prose.** The guard flags any documentation line *beginning* with
`simon `, and a paragraph wrapped so that a quoted comment -- "there is no EDAC
equivalent simon reads, so this is the common path" -- started one. The
verification run for that commit had been made *before* the docs were written, so
the code was green and the prose was never put to the suite. **Run the suite after
the last edit, not after the last edit you think matters.** Rewrapping the line
fixed it; the guard was right.

**A printer's state was in a column the query selected and then ignored.**
`Win32_Printer` describes the same queue twice, and neither column is reliably
the specific one:

| Queue | `PrinterStatus` | `ExtendedPrinterStatus` | `PrinterState` |
| --- | --- | --- | --- |
| Three virtual queues | 3, Idle | 2, Unknown | 0 |
| Brother, on a WSD port | 1, Other | **7, Offline** | **128, `PRINTER_STATUS_OFFLINE`** |

Reading only `PrinterStatus` published the Brother's state as an absence while
the spooler was saying "offline" in two other columns, one of which the query
already selected. `status_from_cim` now takes whichever column names a state --
neither is preferred on principle, because either can be the one holding it --
and 1 (Other) and 2 (Unknown) remain an absence in both.

**Its connection, too.** The port is `WSD-<guid>`: Web Services on Devices,
network printing by definition, named by the port monitor rather than found in
the printer's title. `Win32_Printer.Network` is *false* for it, because the queue
is installed locally -- so the one column the reader consulted said nothing while
the port string said everything.

**A display's connection sat on the second of two nodes for one panel.** The
single LG here enumerates twice in `root\wmi`, once per adapter path. The first
node reports `D3DKMDT_VOT_UNINITIALIZED`, which arrives from WMI as an unsigned
`4294967295` rather than as -1; the second reports 5, HDMI. The join took the
first match, under a doc comment arguing that the ambiguity between two panels of
a model could not reach any value taken from it, since they share their EDID
name, connection family and size. **It reached this one** — and the panels here
are not two panels but one, counted twice. The join now prefers a node that
states a connection, which is a tie-break inside one hardware id rather than a
guess across panels.

**A TPM version that needed no elevation.** `Win32_Tpm` really does answer
"Access denied" on an ordinary account — checked, and the reason for *that*
absence stands — but the security device node `ACPI\MSFT0101\1` is readable by
anyone, and `MSFT0101` is the assigned id of the TPM 2.0 device (`PNP0C31` is the
1.2 one). The friendly name beside it says "Trusted Platform Module 2.0" and is
not what is read: a version taken out of a display string is the substring
guessing this crate removes wherever it finds it, and that string is localisable.
`board.tpm.status` stays absent on purpose — a started driver is not the TPM's
own enabled flag.

**Six GPU reasons named a party that was never asked.** On the AMD integrated
GPU: "driver reports no power telemetry", "driver exposes no enforced power cap",
"driver reports no graphics clock", "vendor publishes no graphics clock ceiling",
"vendor publishes no thermal limit for this adapter", "vendor publishes no
shutdown threshold for this adapter". The Windows AMD backend sets `clocks` and
`power` to `None` without consulting anything; the Intel one does the same and
says so in a comment. The temperature's only source is an OpenHardwareMonitor or
LibreHardwareMonitor WMI namespace, and neither exists unless somebody installed
one — while `atiadlxx.dll` and `amdadlx64.dll`, AMD's own telemetry libraries,
are both in System32 on this machine.

No reading was gained there and none is claimed: the reasons now say what was
read rather than what a vendor supposedly withholds, and stay true for an NVIDIA
card, where NVML *was* asked and declined. **A reason that names a culprit is the
most persuasive kind, and the easiest to get wrong.**

**Four checked and true.** No property of `Win32_Printer` answers CUPS'
accepting-jobs — checked against the whole property list, not a guess at which
one might; both USB nodes without a manufacturer really report a blank one; the
TPM's enabled flag and manufacturer are behind the elevation boundary; and
`gpu.codec.{n}.max_fps` is a deliberate refusal to estimate rather than a missing
read, which the entity and the reader agree on.

### Four reasons that blamed the platform for a column nobody selected

Four more absence reasons checked and **all four false**, on the same method:
group the snapshot's absences by reason, take one, and ask Windows the question
the reason answers. Nineteen readings that were absent are now present, and the
snapshot went from 465 absences to 446.

**A root hub's class was in the property the query did not select.** The USB
reader has a hub branch matching `ROOT_HUB` and `_HUB`, written with a comment
arguing that a structured identifier from the bus driver is not the same thing
as finding "hub" in a display name. It was correct and it was unreachable by the
devices it was written for: Windows leaves `CompatibleID` **empty** on all six
root hubs here and records `USB\ROOT_HUB30` in `HardwareID`, which the query did
not select. The published reason said "no interface class or hub identifier was
recorded for it" while the platform was recording one, three times per hub.

The same reason was wrong a second way for the nine nodes that stay absent: it
asserted a *device descriptor* saying the class is per-interface. A USBSTOR disk
node and a USB4 router have no USB device descriptor at all — they are enumerated
on another bus, and their class, where one exists, sits on a sibling node. The
reason now names the composite parent and the foreign-bus node separately.

**`board.input.{n}.product` was `String::new()` on Windows**, under "reader
returned an empty string" — which is true about the reader and says nothing about
the platform. The entity reads "product name **or numeric id**, whichever the
platform gave"; the id is in the instance path; `usb::parse_vid_pid` already
parses that exact shape; and the Linux reader has published `0x046d` from
`/proc/bus/input/devices` since it was written. Eleven readings, from making
`parse_vid_pid` `pub(crate)` rather than writing a second parser for the two
readers to disagree with.

The `vendor` half is the more interesting one. It was fixed once already, in
`dbe64e0`, by selecting `Manufacturer` and rejecting Windows' parenthesised
driver-package providers — and the fix left five of this machine's six input
devices publishing no vendor at all, because three keyboards report a blank
manufacturer and two mice report "(Standard system devices)". The numeric id was
in their paths the whole time. **A fix that removes a wrong value is not the same
as a fix that produces the right one**, and the absence it leaves behind reads as
settled.

**A camera's driver is `Win32_PnPEntity.Service`** — `usbvideo` on both cameras
here, the direct analogue of the `device/driver` symlink the Linux reader follows
to `uvcvideo`. Not selected; field hardcoded empty; same reason published. That is
the third time in this audit that the answer was a column beside the ones already
being read.

**And an ECC absence blamed on an interface nobody consulted.** Three
`memory.ecc.*` rows carried "no memory controller exposed ECC reporting; the
platform interface exists but enumerated nothing". No interface exists here and
none was asked: `EdacMonitor::scan()` returned an empty overview on every
platform but Linux. The resolver's *other* branch — the one that would have told
the truth — carries the comment "on Windows and macOS there is no EDAC
equivalent simon reads, so this is the common path", and was unreachable. The
module header said Windows read `wmic memorychip`; no code in it has ever run
anything on Windows.

Non-Linux `scan()` now returns an error naming the absence, and on Linux an
absent `/sys/devices/system/edac` is reported as **no EDAC driver loaded** rather
than as a loaded driver finding no controller — a different fact, and the
difference between "this machine reports no ECC errors" and "nothing here counts
them".

`Win32_PhysicalMemoryArray.MemoryErrorCorrection` is 3, "None", on this host, and
it is tempting to publish `memory.ecc.active = false` from it. That answers
whether the modules carry ECC, which is the per-slot entity and is already read;
it does not answer whether a controller is reporting corrections, which is what
these three ask and what the entity's own text warns against conflating.

**One reason checked true, where the fix would have been wrong.** "This entry
carries no USB vendor id; root hubs and virtual devices have none" survives the
hardware ids that just fixed the class beside it: `USB\ROOT_HUB30&VID1022&PID43FD`
does carry a vendor and a product. They are the **PCI** ids of the AMD host
controller, reused by Windows to name a synthetic device. Publishing them in a
USB vendor field would have been a category error arriving as a bug fix, in the
same commit, from the same property.

### An honest reason for an unfinished reader

Two more reasons checked and true — no NVMe data on the USB mass-storage gadget
that is `disk.0` (the three real NVMe drives report everything), and no minimum
core frequency anywhere in `Win32_Processor`. That is **five reasons true and
five false** out of the ten examined.

The audio ones were a third kind. Twelve readings said:

> simon has no mixer binding on this platform, so no level is read
>
> which endpoint the system routes to by default is a COM call this crate does
> not make

Both are *true statements about the reader* and neither is a fact about Windows.
`IMMDeviceEnumerator` and `IAudioEndpointVolume` answer all three questions
unelevated, and nothing else does: `Win32_SoundDevice` has no volume property,
and the `MMDevices` registry tree this module already reads stores endpoint
configuration rather than the live scalar. COM was not a preference; it was the
only source.

Keyed by endpoint GUID — what `IMMDevice::GetId` carries after the container id,
and what the registry trees are already keyed by — so the new reader joins the
existing enumeration instead of replacing it. `master_volume` is the default
*output* endpoint's level: Windows has no system-wide volume, only per-endpoint
ones, so that is what the name can honestly mean. It had been `Some(100)` from
the constructor, published as "Master Volume: 100%" on every machine.

Cross-checked through an independent COM path — `Add-Type` in PowerShell with its
own vtable declarations — reporting render 56% and capture 24%, matching simon
exactly. After the L2 cache count, a reading that merely *looks* right is not
evidence.

**Three guards asserted the old absence, and restating them was the careful
part.** Each was written when nothing read a mixer, and each pinned `None`. The
temptation is to delete them; the error is to keep them, because pinning absence
on a platform that now reads the value pins the defect rather than the principle.
Each now asserts what it was actually defending:

- the master volume *equals the default output's*, rather than being a figure of
  its own — sabotage-verified by restoring `Some(100)`, which it caught;
- a failed setter leaves the previous reading untouched, compared against what
  was read rather than against `None`;
- "only the default endpoint reports a volume and it is exactly 100" — the
  original defect's precise shape — is not a state any mixer produces.

**And one of this crate's own guards caught me.** `cargo fmt` collapsed a
backslash-continued assertion message onto one line, baking the source
indentation into the string: `"...all of them read                  100%"`.
`string_literals_carry_no_collapsed_indentation` exists for exactly that and
named the line. Use `concat!` for multi-line literals here; the continuation form
does not survive formatting.

### The one entity whose rows over-claimed

Two more absence reasons checked and both **true**: "platform exposes no minimum
core frequency" (`Win32_Processor` carries `MaxClockSpeed`, `CurrentClockSpeed`
and `ExtClock`, and no minimum) and "this controller reports no port count"
(`Win32_SCSIController.MaxNumberControlled` is blank on every row here). The
audit is converging: the reasons that were wrong all asserted something the
platform *does* provide through a different column; these two have no source at
all.

Checking the frequency claim turned up something else. This machine reports:

```
cpu.core.0.frequency       derived    5282 megahertz
cpu.core.0.frequency.max   measured   4400 megahertz
```

A current clock 887 MHz above the stated maximum. That relationship is correct
and the entity documents it — *"Rated maximum core clock, as the firmware reports
it. A boosting core can and does exceed it."* 4400 is this part's base clock, not
its 5.6 GHz boost ceiling, and Windows exposes no boost figure.

**The defect was the label.** `measured 4400 MHz` says simon observed the core at
4400. It read a number the firmware states. The entity beside it declares
`Specification`; the resolver called `push_opt` where `push_spec_opt` was already
sitting.

Found by comparing every row's resolved provenance against its entity's
declaration across all 255 entities, and it was **the only one in the ontology
that over-claimed** — on all 24 of its rows.

**No test was added, and that is the interesting part.** `tests/honesty.rs`
checks the opposite direction and argues at length for leaving this one alone: an
`Entity` carries a single provenance and cannot say "varies per row", so a
cluster declares the weakest its rows can carry and over-delivers on the rest.
`cpu.core.{n}.frequency` does exactly that — Linux measures `scaling_cur_freq`
where Windows derives. A blanket check would fail on Linux for a perfectly honest
entity.

That reasoning holds. It simply does not cover this case: `frequency.max` is not
a cluster, because no platform measures a rated figure, so every row over-claimed
uniformly. The harm is the one that test's own documentation names — *"a
spec-sheet inference wearing a measurement's authority"* — arriving from the side
it deliberately does not guard.

**When an existing test's reasoning explains why it does not catch something,
read the reasoning before overriding it.** The rule here was right; the case was
outside it. Adding a stricter test would have broken a legitimate entity on a
platform I cannot run.

### Not guessing was right; not looking further was not

Two readings said `reader returned "Unknown"` for `board.input.N.interface`, and
the reader had earned them honestly. A `HID\...` instance id names a device
*class*: the same prefix covers a USB mouse, a Bluetooth keyboard and an I2C
touchpad. An earlier note in that file said exactly that while the code reported
USB anyway, and the fix at the time was to stop guessing.

Stopping was correct. Stopping *there* was the part left undone. Windows records
`DEVPKEY_Device_Parent`, and the parent carries the transport in its own
enumerator prefix:

```
HID\VID_046D&PID_C548&MI_01&COL01\...   the mouse
USB\VID_046D&PID_C548&MI_01\...         its parent
```

Walking up until an enumerator is recognised turns a refusal into a reading, and
the traversal was already written — `CM_Get_Parent`, the same walk the USB speed
reader needed in `2c80d66`. Six of six input devices now report a transport where
three reported none.

`HID` is deliberately absent from the enumerator table and a test asserts it
resolves to nothing, because if it ever resolved the walk would be pointless. An
enumerator the table does not recognise still answers `Unknown`.

**A correct refusal can still be an unfinished reading.** The absence here was
well-reasoned, documented, and defended against a previous wrong guess — which is
exactly what made it invisible. Three of the five defects in this audit were
absences that had been *argued for* in a comment; the argument was sound about
the evidence in hand and silent about the evidence one call away.

Also worth recording as a process note: while adding the helper I inserted it
between `device_vendor`'s doc comment and `device_vendor`, so the doc silently
documented the wrong function. `cargo` says nothing about this. Check what a doc
comment sits above after inserting anything near one.

### Two more absences that were not

The audit continued into the smaller reasons, and both of the ones I checked
were false.

**"the CPUID family/model/stepping triple was not read on this platform"**, on
three readings, with the reader carrying a comment asserting the same:

```rust
// WMI does not expose the CPUID triple. This returned (0, 0, 0),
// which is not a triple any x86 CPU reports.
```

`Win32_Processor.Description` is the string `AMD64 Family 26 Model 68 Stepping 0`.
The query selected four other columns. `PROCESSOR_IDENTIFIER` carries the same
text — and this file was already reading that variable, two functions further
down, to guess feature flags from. Parsed by keyword rather than position, and
all three must parse or the answer is `None`: a family without a model invites
matching a CPU against the wrong microarchitecture, which is why the three travel
together at all.

**"the platform reported no line size for this cache"**, and its neighbour about
sharing. This module's own documentation opens with *"Uses WMI
(`Win32_CacheMemory`) or `GetLogicalProcessorInformationEx`"* — and only the
first was implemented. It is the weaker source three ways over: `LineSize` is
declared on the class and blank on every row, `Associativity` is a CIM
enumeration rather than a way count (7 means 16-way, and it was published as 7),
and it aggregates per level so the separate L1 data and instruction caches cannot
be recovered from it at all.

The Win32 call returns one record per physical cache. This machine went from 3
aggregate rows to 38 real ones — 24 L1, 12 L2, 2 L3 — with line sizes, true
associativity, and `shared_cpus` naming the SMT pairs and the two CCX groups.

**And a bug I put in, caught by counting rather than by testing.** The walk over
those variable-length records required
`size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>()` bytes to remain in the
buffer. That union is sized by its *largest* arm, which is bigger than a cache
record, so the final record was skipped. The reader found **11 of 12 L2 caches**.

Everything about that output looked right. The line sizes were 64 bytes, the
sharing lists were sensible SMT pairs, the L1 and L3 totals were exactly correct,
and the tests passed. `cpu.cache.l2` read 11 MB, which is only wrong if you
happen to know the part has twelve cores and multiply. **A plausible number is
the failure mode this crate exists to catch, and I shipped one into my own
verification step** — the fix came from counting instances against the hardware,
not from anything the toolchain could have told me.

### Auditing 491 absences against the platform

The DIMM voltage suggested a method, so I applied it to the whole snapshot: take
every `unavailable` reading, group by reason, and ask the platform whether the
reason is true. 491 readings, 30 distinct reasons. Four checked in detail, and
the split is the useful part — **two were true, two were not.**

| Reason | Count | Verdict |
| --- | --- | --- |
| "Windows reports link training nowhere simon can reach unelevated" | 164 | **False** |
| "the platform reports no NUMA affinity for this device" | 64 | True — 0 of 64 PCI devices expose the property |
| "driver reports no negotiated link rate for this interface" | 11 | True — every one is a WAN miniport or disconnected adapter, `0 bps` |
| "reader returned an empty string" | 19 | **False for 1 of 6 checked** |

**The link-training claim was refuted by the same document that made it.** 23
devices in that snapshot report `link.speed` as `measured`, at 16.0 and 32.0
GT/s, read unelevated through the cfgmgr32 property store. simon reaches link
training on Windows; it does so 23 times in the file that says it cannot.

Counting what Windows exposes gives 23 of 64 — matching simon exactly, so nothing
was being missed — but the 41 without include several `PCI Express Downstream
Switch Port` and `PCI standard host CPU bridge`. Those *are* PCIe, so the
alternative clause ("the device is not PCIe") was wrong too. Windows populates
the link properties for **endpoints** only. The reason says that now, with the
count behind it. No reading changed; only the explanation was false.

**And a manufacturer nobody asked for.** `Win32_PointingDevice.Manufacturer`
reports "Microsoft" for this machine's mouse. The query selected Name,
Description, DeviceID and Status, then the code set `vendor: String::new()`.

The interesting half is what stopped this being a one-line fix. **Windows fills
`Manufacturer` on many device nodes with the driver-package provider rather than
the hardware vendor**, marked by convention with parentheses: "(Standard system
devices)", "(Standard keyboards)". Those name who wrote the inbox driver.
Publishing one as a vendor is precisely the "Unknown-as-a-value" defect this
crate has caught in five entities across three releases — a real-looking string
standing in for an absence — arriving through a channel `push_str_as` cannot see.
The reader rejects them; the resolver could not have.

`Win32_Keyboard.Manufacturer` is genuinely blank on every keyboard here, so
keyboards keep their absence — now one the reader observed rather than assumed.

**A reason is a claim, and claims can be checked.** This crate spends real effort
writing absence reasons that explain themselves, which makes them *persuasive* —
and a persuasive wrong explanation is worse than a blank one, because it stops
the next person looking. Two of the four I checked were wrong, and one had been
contradicted by its own neighbours in every snapshot ever taken.

### A reading the query never asked for

Checked the ontology's DIMM rows against WMI directly, rather than reading the
code that produces them. Two defects, and the first is the more interesting.

**`memory.dimm.N.voltage` said "SMBIOS reported no operating voltage" on a
machine whose firmware reports one.** The Windows reader had:

```rust
voltage: 0.0, // WMI doesn't provide this easily
```

`Win32_PhysicalMemory.ConfiguredVoltage` is exactly that figure, in millivolts.
The PowerShell query simply did not select the column. And then the system
behaved *exactly as designed* on top of a value that was never fetched: the
resolver saw a zero, correctly decided a DIMM cannot operate at zero volts,
and published a confident, specific, well-worded sentence about what the
firmware had reported. Every layer did its job. The reading was 1.1 V all along.

```
before: memory.dimm.0.voltage  unavailable  — SMBIOS reported no operating voltage
after:  memory.dimm.0.voltage  specification  1.1 volts
```

against `Get-CimInstance Win32_PhysicalMemory` reporting `ConfiguredVoltage
1100`. The field is `Option<f64>` now, so absence stops travelling as a zero to
be decoded three layers away — `profile/memory.rs` was doing that decoding too,
with its own comment explaining the SMBIOS "zero means unknown" convention, in a
different module from the parse that produced it.

**And DIMM speeds were published with unit `count`.** They are megatransfers per
second. `5600 count` hands a consumer a number and nothing else, in a crate whose
ontology exists so that a unit travels with its value. `Unit::MegatransfersPerSecond`
now exists, mapped to QUDT `NUM-PER-SEC` rather than to a frequency — a transfer
is a bus operation, not an SI quantity, and MT/s is a frequency only if you assume
one bit per transfer, which is the exact assumption double data rate breaks.

**An absence reported for the right reason can still be wrong.** Nothing in the
code was sloppy: the comment stated a belief, the resolver validated correctly,
the reason string was accurate about what it had been given. The defect was one
column missing from a `SELECT`, and the entire careful apparatus downstream
turned that into a plausible, well-argued falsehood. **Checking the code against
the machine is not the same as checking the code**, and only the first would have
found this.

### Following `*_or_zero()` to the end

Having named it a defect generator, I followed every call site. Eleven of them,
across seven surfaces, all reached by the same helper. The full list is in
`162825a` and `3073bf3`; three are worth keeping.

**A health check that disappeared instead of failing.** `health.rs` gated its
swap check on `total_or_zero() > 0`, so a machine whose pagefile could not be
read produced *no swap check at all*. A health report with a check missing does
not read as "unknown" — it reads as a report where that check passed, because
that is what a clean report looks like. `HealthStatus::Unknown` is a variant of
this crate's own enum, documented "Unable to determine status", and it was not
used anywhere in the file.

**A summary panel showing `SWAP 0%`.** The GUI's `QuickLookPanel` took
`swap: f32`, and its caller recomputed the percentage from `*_or_zero()` rather
than calling `swap_usage_percent()` — which sits on the same struct, already
returns `Option`, and already handles a zero total. The recomputation existed
*only* because the destination could not hold the answer. That pattern repeated
in `MemoryState`, in the GUI, and in the TUI: three separate hand-rolled
divisions, each subtly different, all shadowing one correct method.

**And the same block, twice, in the same file.** The CLI's swap display appears
in two functions, and both had the identical three-cases-into-two collapse — in
use, none configured, not reported — with the third printing nothing at all. Both
needed the same fix because both were written the same way.

Ten call sites remain and every one is now inside a guard that has already
established the value was read. **Checked, not assumed** — I walked each one and
confirmed the guard, rather than reasoning that the ones I had not touched were
probably fine.

**Two lessons, and the second is the uncomfortable one.**

A helper that answers a question the caller did not ask — "what if it is absent?"
— will be reached for at every site where the honest answer is inconvenient, and
it reads as harmless at all of them.

And: **a correct method already existed.** `swap_usage_percent()` returned
`Option`, handled the zero-total case, and was sitting on the struct the whole
time. Four call sites recomputed it by hand instead, each reintroducing the
defect it was written to avoid. Writing the careful version is not enough if the
convenient wrong one is still in reach.

### Six power readings unwrapped to zero, and a lie about the machine

The last two fixes suggested a detector, so I built it: **a struct where some
numeric fields are `Option` and the rest are not.** That mixture is the signature
of a type that learned to express absence for one field and never for its
neighbours. 134 structs match. Most are legitimate — ids, counts, flags, things
that really are always known. Two were not.

**`gpu::traits::Power` had one `Option` field and six bare ones**, and the
`Option` one was documented *"(if available)"* — which quietly implies the others
always are. They are not; every one is a driver query that can decline. All three
backends read them **as `Option`** and destroyed the absence one line later:

```rust
current: current.unwrap_or(0.0),
limit: limit.unwrap_or(0.0),
default_limit: default_limit.unwrap_or(0.0),
```

Then the absence was *laundered back into a reading*. `GpuPower::default_limit`
is `Option<u32>` — a type that can say "unreported" — and `gpu/mod.rs` filled it
with `Some((p.default_limit * 1000.0) as u32)`. A card whose default limit NVML
declined published `Some(0)`: not merely a zero, but a zero explicitly marked
present.

Two more in the same constructors. The AMD and Intel readers wrote
`average: if current > 0.0 { Some(current) } else { None }` — the **inverse**
mistake, a card genuinely drawing zero watts reported as having no average
reading at all. And Intel's `min_limit` was a hardcoded `0.0` on a path that
reads no minimum whatsoever.

**And the GUI told the user a fact about their machine that it had not
established.** It took `mem.swap.total_or_zero()`, branched on `> 0.0`, and
printed:

> No swap configured

`SwapInfo` carries `Option` precisely so an unread pagefile is distinguishable
from an absent one, and Windows swap comes from a WMI query that can fail. The
sentence is about the machine; the truth available was about the reading. It now
says "Swap not reported by this platform" when that is what happened. **That is
the fourth site this session where `*_or_zero()` throws the distinction away at
the point of use**, after the agent tools (`a50b77d`) and the recorder
(`0801f11`) — a helper that exists to be convenient at exactly the moment being
careful matters.

**The examples had been hiding it.** Several guarded on `power.current > 0.0`
before printing — which was how they told "unreported" from "reported" back when
the type could not say. Every one of those guards was a workaround for this
defect, sitting in plain sight in the code that demonstrates the API.

Verified: two discrete cards at 19.1 W and 12.0 W against 450 W limits, and the
integrated adapter's power correctly unavailable.

**A convenience method named `*_or_zero()` is a defect generator.** It is
correct at some call sites and catastrophic at others, it reads as harmless, and
nothing distinguishes the two cases at the call site. Four sites this session,
across three surfaces, each written by someone reaching for the shortest thing
that compiled.

### GPU utilization, and an unreadable card advertised as idle

The other half of the same type. `556a471` took `GpuMemory` to `Option` and left
`GpuDynamicInfo::utilization` for a separate pass, which is this one; the shape
is identical. Two backends returned a literal `0` having read nothing at all, and
NVIDIA wrote:

```rust
utilization_rates().ok().map(|u| u.gpu as u8).unwrap_or(0)
```

An idle GPU and an unreadable one are the same number and different facts. **The
difference matters because two consumers were acting on it.**

`GpuState::is_idle()` returned `true` for a device with no counter — an
unreadable GPU advertised as available work capacity, to whatever schedules work.
And `avg_utilization()` averaged those zeros in, dragging a fleet mean down
toward spare capacity that may not exist. Both now ignore devices that report
nothing rather than counting them as idle. That is the third missed alarm of this
exact shape in three commits, after `is_downgraded` in `3656368` and
`memory_usage_percent` in `556a471`.

**Two readers turned out to know more than they could say.** `tuning/serve.rs`
already carried the comment *"`None` … rather than as 0%"* sitting over code that
had no way to produce a `None`. It can now.

And the Intel Linux path derives utilization from the **clock ratio**, which is a
proxy rather than a measurement: a throttled but saturated GPU reads low, and one
parked at maximum clock reads busy. That is now stated at the site, along with
the `i915` PMU engine-busy counters it should be reading instead — recorded
rather than quietly kept, because the number is presented as `utilization`
everywhere downstream and nothing said it was inferred.

`platform/linux/gpu.rs` answered `0.0` both when the sysfs `load` node was
missing and when it failed to parse — alone among its siblings, every one of
which uses `path_exists … else None` two lines above it.

Verified on this machine: three adapters at 2%, 0% and 0% — genuine zeros on idle
cards, still perfectly distinguishable from absence.

**Two of the sites were inside `cfg(target_os = "linux")` blocks.** Nothing on
Windows compiles them, `cargo clippy --all-targets` does not see them, and the
full test suite passes without them. Only the Linux cross-check finds this class,
and in a 30-site change it found two. Run all three targets before believing a
sweep is finished.

### A type that could not say "not reported"

The same sweep, one layer down. Every field of `GpuMemory` was a bare number,
and all four GPU backends independently filled them with zeros when they had
nothing to read. Two of them said so in a comment while doing it:

```rust
used: 0, // Not available from powermetrics            (apple.rs)
memory: GpuMemory { total: 0, used: 0, free: 0, .. }   (amd.rs, intel.rs)
```

That is the defect in one line. The code *knew* the value was unavailable and
wrote the most plausible-looking number it had, because the type gave it no way
to say anything else. A GPU reporting 0 bytes of memory is not a reading any real
adapter produces — and it reached the TUI, the GUI, the Prometheus exporter, the
observability API, the ontology and the recorded database labelled `measured`.

Note what surrounds those lines: in the very same constructors, clocks, power and
temperature are all `Option` and all correctly `None`. The backends were not
careless. They were exactly as honest as their types allowed, field by field.

**Three of the 96 sites were worse than a zero.**

| Site | What it did |
| --- | --- |
| AMD + Intel WMI readers | Where the adapter's capacity was not reported, the total fell back to *the used figure itself* — so a card whose capacity WMI did not know read as exactly **100% memory utilisation, permanently, on every scrape** |
| `TraitGpuAdapter` | Divided by `total.max(1)`, turning a missing total into "0% of one byte" rather than into an absence |
| `GpuState::memory_usage_percent()` | Returned `0.0` for an unread device, and `health_status()` tests it against 95% — so an unread GPU could never raise the memory warning |

That last one is the same missed alarm as `is_downgraded` in `3656368`, found the
same way and one release apart.

`GpuMemory::unreported()` and `from_total_used()` mean the percentage is derived
once instead of in four places that each got the zero-denominator case wrong
differently. Every surface now says what it knows: the ontology reports the two
figures independently rather than gating both on `total > 0`, Prometheus and the
served endpoint omit an absent gauge, the histories contribute no sample rather
than a zero sample, the TUI and GUI draw a dash, the CSV exports write an empty
field, and the recorder stores `None`.

Verified on this machine: three adapters, all still reporting real figures, with
874 tests, clippy, doctests and both cross-targets green.

**A 96-site type change is the safe kind of large.** Every one of them was found
by the compiler, so nothing changed behaviour silently and nothing could be
missed — including the two that only the Linux cross-check could see, sitting
inside `cfg(target_os = "linux")` blocks and invisible to every Windows build.
The risk in a change like this is not the size; it is the sites the compiler
*cannot* find, and there were none, because the fix was to a type rather than to
a value.

**And the honest limit: this is verified where the readings succeed, not where
they fail.** All three adapters here report their memory. The paths that now
return `unreported()` are the ones nobody has watched execute — the Apple reader,
the Intel Linux reader, and the two WMI fallbacks. They are simple, and they are
unwitnessed.

### Six recorded columns that wrote zero for a failed read

The sweep that found the six uncalled readers had a second half nobody had run:
the same filter — a function returning a bare number with a zero fallback — over
the code that *is* called. 46 hits, most of them legitimate (clocks, `is_*`
predicates, the deliberately-named `*_or_zero`). Six were not, and they were the
six that end up in a file on disk.

`simon record` writes a row per tick whether or not the readers succeeded, and
that row is read back later by someone who was not there:

| Column | What it stored for a failed read |
| --- | --- |
| `cpu_percent` | `0.0`, from `Snapshot::cpu_utilization()` |
| `cpu_per_core` | `100.0 - core.idle.unwrap_or(100.0)` — a core with no idle reading recorded as 0% busy |
| `memory_used` / `memory_total` | `0`, from `.unwrap_or((0, 0, 0, 0))` |
| `swap_used` / `swap_total` | `used_or_zero()`, discarding at the point of storage the `Option` that `15a60ab` added for exactly this |

None of that is a visible gap. `0.0%` CPU beside 0 bytes of memory is a specific,
plausible claim that the machine was idle and empty at that moment — and the
readback divides by `memory_total`, so an unread tick printed `NaN%` next to a
confident `0.0%`.

**The same function already gets this right three times over.** The GPU columns
push `None` for an unqueried device, under a comment reading *"it is not idle,
cold and drawing no power"*. The per-process I/O fields are `None` because zero
*"reads back as an idle process"*. The network rates are `None` rather than `0`
because *"a recording that starts with a zero teaches whoever reads it back that
the network was idle"*. The CPU and memory lines sit in between, doing the thing
all three comments warn about.

All six are `Option` now. Both readbacks print "not read" where the GPU rows
already printed "no sensor", and the CSV writes an empty field where the two
network columns already did. `DB_VERSION` 5 → 6: as with the two bumps before it,
the layout moving matters less than the values changing meaning, and a reader
that took version-5 rows as version-6 would keep reading those zeros as
measurements.

Verified by recording and reading back — 21 rows, real CPU and memory throughout,
network rates empty for the first samples and populated once the collector had
two — plus a round-trip test that writes an all-absent tick and asserts it comes
back absent.

**A file full of careful `None`s is not evidence that the writer is careful.**
Three separate fixes had passed through this function, each fixing its own
column and each leaving a comment explaining the principle, and the columns
beside them stayed wrong through all three. Fixing an instance is not the same as
sweeping the site, and the comments left behind made the site *look* audited.

### The negotiated USB speed, and nothing running at super speed

Open-work item 2, and the item was wrong about the hard part.

`usb.{addr}.speed` was absent on every device. Windows genuinely reports it
nowhere a query can reach — not a PnP property, not in `Get-PnpDeviceProperty`,
and no class in `root\wmi` or `root\cimv2` carries a field for it. All three
were checked first, because the item asked for a ground truth before trusting
any result and the answer is that none exists.

It comes from `IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX` against the parent
hub, addressed by the device's port. **The item said that port comes from
`LocationInformation`, which only 14 of this machine's devices carry, and called
the other 25 blocked on "the parent traversal".** It comes from `CM_DRP_ADDRESS`,
which every node has. The blocker was a wrong guess about where a number lives,
and it had been sitting in the list for weeks.

The traversal that *is* needed is different and smaller. Two passes:

1. A node whose parent is a hub is read directly.
2. A node whose parent is not a hub is an interface of a composite device —
   `...&MI_02\...` — and negotiates nothing of its own. Its link belongs to the
   device it is one function of, so it inherits. **That is 18 of 38 nodes here**,
   nearly half the tree, sitting exactly one step from the answer.

`USBSTOR` is enumerated alongside `USB`, because an external drive is where this
diagnostic earns its keep.

Measured, 33 of 41 devices answered:

```
17 measured full
16 measured high
 8 unavailable
```

The eight are six root hubs and two USB4 nodes. None sits on an upstream hub
port, and that is the true answer rather than a gap — a speed is a property of a
*link*, and they have none above them. The resolver's absence reason used to say
"this reader does not ask for the negotiated speed … obtainable and
unimplemented"; it now says what is actually the case, because a stale reason is
a lie the same as a stale number.

**Nothing on this machine negotiates super speed.** The name heuristic this
replaces — `USB3` or `xHCI` in the PnP path meant `Super` — called six devices
super. Two of those declare `bcdUSB 3.00` and are running at high speed: a USB 3
device on a USB 2 link, which is *precisely* the wrong-cable case the field
exists to expose and *precisely* where the heuristic was confidently wrong. The
old reader would have told a user their cable was fine.

The ioctl's `CTL_CODE` carries `FILE_ANY_ACCESS`, so the hub handle opens with
zero desired access and none of this needs Administrator — read off the code
first, per the standing lesson from `IOCTL_ATA_PASS_THROUGH`.

**An open-work item can be wrong about why it is open.** This one had a specific,
plausible, checkable claim in it — the port comes from `LocationInformation` —
and that claim was what made the work look large. Checking it took one
`Get-PnpDeviceProperty` call. Before believing an item's account of its own
blocker, verify the blocker.

### One WMI connection instead of N, and the counters it was hiding

Went to close the served endpoint's two `_total` gaps, which needed cumulative
disk counters the pipeline `Snapshot` did not carry, and found out why it had
never carried them. Measured on this four-drive host:

| Path | Cost |
| --- | --- |
| `logical_drives` (capacity only) | 0.5 ms |
| `enumerate_disks` | 49 ms |
| `io_stats`, looped per disk | **8000 ms** |
| `all_io_counters`, batched | **9.9 ms** |

`read_io_counters` opens its own WMI connection on every call, and the
connection is the entire cost — the query itself is nothing. That is why
`collect_disks` takes the capacity-only path on Windows, which is why the
snapshot had no counters, which is why the metric could not be published. **The
performance problem and the missing metric were the same fact**, and neither was
visible from the other end: the gap looked like a missing feature and the cost
looked like "WMI is slow".

800× for identical values. The Prometheus exporter was paying the eight-second
version on *every scrape*.

With the cost gone the counters go into the snapshot as `disk_io` — a separate
list rather than fields on `DiskSnapshot`, because the two are keyed differently.
`DiskSnapshot` is per filesystem and these are per physical drive; on Windows one
drive can carry several letters and one letter can span drives, so attributing a
device counter to a mount needs a partition map nobody has written. Labelling by
device sidesteps it and is what the metric means anyway:
`simon_disk_read_bytes_total{device="PhysicalDrive0"}` is a property of the
hardware, not of a mount point.

Then `cpu_temperature_celsius`, the last gap, by the same shape: the reading goes
in the snapshot and the recorder publishes it, because `record_snapshot` is pure
over a `Snapshot` and that purity is what makes it testable. Collected on the CPU
stage — ~2 ms after a one-time ~365 ms initialisation, with the stage measured at
1.2–3.4 ms per tick after the first, unchanged.

**Both pinned gap lists are empty.** Every metric the bundled dashboards query is
published by the renderer that serves them, and either list failing to be empty
fails the build.

One honest limit on that: this machine reads **zero** CPU temperature sensors
through all four Windows paths `hwmon` tries, so what is verified here is the
absence — that a machine with no sensor publishes no series rather than a `0`.
The populated path is covered by a unit test against a synthetic sensor and has
never met real hardware that reports one.

**A cost measurement is a design tool, not a benchmark.** Three sessions of work
had routed around this WMI cost — the capacity-only path, the missing snapshot
field, the unpublishable metric — without anyone timing it. Two numbers, 0.5 ms
against 8000 ms, explained all three at once and pointed at the one-line cause.

### What the split guard immediately found

Splitting the dashboard check paid for itself in the next hour. Three defects,
all in the code the combined test had been declaring complete.

**`simon_network_rx/tx_bytes_total` carried a rate on the served endpoint.**
`record_snapshot` recorded `total_rx_rate()` — bytes/sec, summed over interfaces,
unlabelled — under a `_total` counter name. The bundled fleet dashboard plots
`rate(simon_network_rx_bytes_total[5m])`, so it was taking the rate of change *of
a rate*: near zero under steady traffic, and a spike shaped like the derivative
of the load rather than the load itself.

The library exporter has published the true cumulative counter under that exact
name all along, per interface. The two publishers disagreed about what the name
meant, and the one reachable over HTTP was the wrong one — which is exactly the
shape of gap the split was written to expose, found on the first look.
`NetSnapshot` already carried `rx_bytes`/`tx_bytes`, so nothing had to be added:
the counters are labelled per interface now, and the rates moved to
`_bytes_per_sec`. Labelled rather than summed deliberately — a sum over counters
jumps backwards when an interface disappears, and Prometheus reads that as a
reset on the whole series.

**`SystemStats::total_processes` was a bare `u32` that only Linux assigned.**
Windows and macOS returned `0`: no processes at all, on a machine running several
hundred. It is `Option<u32>` now, and Windows answers it from
`PERFORMANCE_INFORMATION.ProcessCount` — a field of the struct this crate was
*already calling* for the system file cache, so the reading costs nothing beyond
noticing it was there. Measured 527 against 518 from both `Get-Process` and
`Win32_Process`, with cargo's own process tree accounting for the difference.
(`\Objects\Processes` reads 671; it counts something else and is the outlier —
worth knowing before anyone "corrects" this against it.) `running_processes` is
`Option` too: only Linux has `procs_running`.

**And a load average invented from a single CPU sample.** Following the process
count into `observability/api.rs` turned up this, on the Windows arm of
`collect_system_load`:

```rust
let load = (usage as f64 / 100.0) * cores;
SystemLoadMetrics { load_1: load, load_5: load, load_15: load, .. }
```

Windows has no load average. This synthesised one from one instantaneous
utilisation reading and served it, through the observability API, under three
names that each promise a different time window.

It is wrong in three ways, and each removes the reason the metric exists. It
cannot exceed the core count, so queue depth — the whole point, the part that says
how far past capacity the machine is — is precisely what it cannot express. It has
no history, so the three windows that separate a spike from a trend were
byte-identical. And it counts busy CPUs rather than waiting tasks, so a machine
wedged on I/O, the case load average exists to expose, read as idle. `None` on
Windows now; the Linux arm's `parse().unwrap_or(0.0)` went too, since a
`/proc/loadavg` that will not parse says nothing about how busy the machine is.

Then the cheap half of the open-work item itself: `PrometheusExporter` learned
the four metrics it had never been taught, all of which it could always have
read. Driven rather than grepped for — `simon_cpu_frequency_mhz 5148`,
`simon_process_count 542`, and the two load averages correctly absent. Its pinned
gap list is empty and the test fails if it regains an entry.

**A test that has just been made stricter is the best moment to go looking.**
These four sat behind a check that read 24/24 and had read 24/24 for as long as it
existed. Nothing about the code changed to make them findable; the question got
sharper, and they were the answer.

### Two renderers covering for each other, and a test that vouched for itself

Open-work item 1 had asked for this and given the reason: the dashboard guard
accepted a name published by *either* renderer, so a gap in only one of them was
invisible. Splitting it per publisher turned up more than the item predicted.

| Publisher | Coverage | Missing |
| --- | --- | --- |
| Served endpoint (`http_server.rs`) | 21/24 | `cpu_temperature_celsius`, `disk_read_bytes_total`, `disk_write_bytes_total` |
| Library exporter (`prometheus.rs`) | 20/24 | `cpu_frequency_mhz`, `load_average_1m`, `load_average_5m`, `process_count` |

Combined, that reads 24/24. Neither publisher is complete, their gaps do not
overlap at all, and each was silently covering the other's — while only one of
them is reachable over HTTP. `/api/v1/metrics/prometheus` serves
`MetricCollector`, filled by `record_snapshot`; a name that only
`PrometheusExporter` knows is not on the wire, however green the test was.

The library exporter's four are the more embarrassing half, because they are not
snapshot limitations. `PrometheusExporter` collects from the system directly and
could read every one of them. They are simply metrics nobody taught it, and no
test ever asked.

**And the split found the comment-satisfies-the-test hole a second time, wearing
a better disguise.** The first version of this guard searched raw source and
passed while the defect was live, because the comment explaining the defect
contained the name. Comments have been stripped ever since. But `http_server.rs`
contains this, in a test:

```rust
assert!(!text.contains("simon_disk_read_bytes_total"));
```

— an assertion pinning that the name is *not* published. A source search read the
string literal as a publisher, so the metric counted as covered on the strength
of a test asserting it was missing. The served endpoint measured 22/24 until
`#[cfg(test)]` modules were stripped too, at which point it read the true 21/24.

Each publisher's gaps are now pinned as an exact list, and the check fails in
**both** directions: a new gap fails, and so does closing one without pruning the
entry. Sabotage-verified each way. That second direction is not decoration — an
exemption list nobody prunes stops describing anything, and this file has already
carried one coverage figure that was stale for several commits before I re-measured
it.

**"The output is not published anywhere" and "no code publishes it" are different
questions, and so are "the name appears in this file" and "this file emits it".**
Every wrong version of this test has been a wrong answer to which question was
being asked. Source text answers the second pair only after everything that is
not publishing code — prose *and* tests — has been taken out of it.

### Six uncalled readers that answered zero for unknown

The type sweep did not generalise to functions, and it is worth saying why
before the next person tries it. 189 of the crate's 1670 public functions are
called nowhere inside it — but for a monitoring library that is simply its API
surface. `cpufreq::set_turbo`, `fan_control::set_curve`, `memory_management::purge_memory`:
these exist to be called by a consumer, and a guard demanding an internal caller
would be demanding the library use itself.

So the filter had to be narrower, and the honest one is: of the uncalled
functions, which **return a bare number and fall back to zero**. Seven. Six were
real.

**`PcieLinkSpeed` was the worst, because the zero propagated.** All three lookups
on the enum mapped `Unknown` to zero — `gen_number() == 0`, `transfer_rate_gts()
== 0.0`, `per_lane_gbps() == 0.0` — and `Unknown` is what `from_sysfs` returns for
*any* string it fails to parse. There is no PCIe Gen0. From there it spread into
two places that matter:

| Consumer | What the zero did |
| --- | --- |
| `max_bandwidth_gbps` | Returned 0.0, and it is *summed* across devices into `PcieBandwidthSummary::total_bandwidth_gbps`. A device of unknown speed contributed nothing to a figure presented as the bus's aggregate bandwidth, and the total looked complete |
| `is_downgraded` | Compared generation numbers with `Unknown` as 0, so it was wrong in both directions at once |

That second one is worth spelling out, because it is a false alarm and a missed
alarm in the same three lines. A device whose *maximum* speed was unreadable
compared as Gen0 and so could never exceed the current speed — never downgraded,
whatever was happening. A device whose *current* speed was unreadable compared as
Gen0 against a known Gen4 maximum and was reported as degraded — a degradation
alarm about a link that was never measured. And `PcieMonitor::downgraded_devices`
put that device in the list a caller acts on.

Everything is `Option` now. `is_downgraded` decides on width first, which is the
cheaper signal and settles the case on its own — a link narrower than its maximum
is downgraded whatever the speeds turn out to be — and only consults speed when
width says no, which is the only case that needs both to be known. The summary
gained `unreadable_devices` rather than quietly excluding them, so a reader can
see that the other two counts are partial.

**`BlockDeviceIo`'s three stats figures**, fixed for the zero and then for
something worse than the zero. They returned 0.0 when the busy-time denominator
was zero, which happens both when a device has genuinely served no reads and when
the counters were never populated — every platform but Linux, where `IoStats` is
constructed with zeros. But going in to fix that made me read what they compute,
and the names are wrong twice over: `/sys/block/*/stat` counters are cumulative
since boot, so these are *lifetime averages*, not rates, and will barely move
under a burst of load; and the denominator is time *spent* doing I/O rather than
elapsed time, so the figure is IOPS-while-busy, which on a mostly idle device is
far above its actual IOPS. Both now documented at the method, with the
instruction to read `IoStats` twice and difference it for a real rate.

**`RackInfo`'s two utilisation figures** returned 0.0 when no capacity was
configured. "This rack is drawing 0% of its power budget" is the most reassuring
possible answer to a question nobody supplied the inputs for, and it was the
common case: `max_power_watts` defaults to `0.0` in the builder precisely because
it cannot be known without being told.

The seventh, `streaming::subscription_count`, counts a collection. Zero is a real
answer to how many subscriptions there are, and it was left alone.

**Being uncalled is not the defect; it is the reason the defect survived.** Every
one of these had been read past by the sweeps that fixed the same pattern
everywhere else in the crate, because those sweeps followed values to the
surfaces that display them and these values reach no surface. The `Unknown => 0`
arm is the single most repeated shape in this file's history, and it was still
sitting in three consecutive match arms of a published enum.

### 26 public types the crate declares and never produces

Took the lesson from the previous commit at its word and swept every public
`struct` and `enum` in the crate — 885 of them — for ones mentioned nowhere but
their own declaration. Eight. Following those transitively through the fields
that only they reference: 24 of the 32 types in `ai_api::types`, 391 of its 619
lines, plus `EdacEdgeType` and `SiliconSnapshot`.

The `ai_api` half is the one that matters, because it is published.
`ai_api::types` is a `pub mod` re-exported with `pub use types::*`, so a consumer
reading the docs finds `CpuDetails`, `GpuDetails`, `DiskDetails`,
`ProcessDetails`, `NetworkInterfaceDetails` and eighteen more, under a module
header reading *"data structures for tool results"*. No tool returns one. I
checked whether they were at least a deserialisation contract for the JSON the
tools do emit, and they are not:

```
CpuDetails      { model, physical_cores, logical_cores, cores,
                  total_utilization, frequency }
get_cpu_status  { core_count, total { user_percent, system_percent,
                  nice_percent, idle_percent, usage_percent }, model }
```

Two different documents about the same subject. The tools build their results
with ad-hoc `json!` and always have.

What makes this an honesty defect rather than tidying is the *shape* of the dead
types. They are flat and non-`Option` — per-core clocks, power draw,
temperatures, SMART attributes, packet counters, bandwidth rates — declaring as
unconditionally present exactly the readings the last hundred commits have
established are routinely unavailable on a real machine. `BandwidthInfo` still
carried the bare `f64` rate fields that `e425908` removed from every network rate
in the crate that something constructs. This is a published description of a
machine simon cannot actually see, and the first person to wire one up would have
imported the fabrication wholesale rather than met it as a fix.

Deleted. `tests/unreachable_types.rs` keeps the count at zero, with **no
allowlist** — a public type appearing nowhere but its declaration is either dead
or an unkept promise, and both deserve a deliberate decision rather than an
exemption line. Comments are stripped before counting, because every type's own
doc comment names it and would otherwise vouch for it; that is the third guard in
this file to need that, after the dashboard one that a comment satisfied.
Sabotage-verified by adding a `SabotageProbe` struct with an `f64` rate field,
which the test named in its failure.

Breaking, and 6.0.0 — where the constructor renames and the `SwapInfo` `Option`
fields are landing — is the release for it.

**Two sweeps in a row have now found their remaining defects in code that does
not run.** Grepping for a pattern finds the code that executes; the code that
does not execute keeps whatever shape it was born with and stays wrong silently.
Both of this crate's last two fabrications were in unconstructed types, and
neither was found by looking for fabrications.

### A section of the state document that was never filled in

The lesson from `f64f5ea` was that every place a rate is *stored* is worth
checking, so I swept the crate for rate-shaped fields. Two were still declared
`f64`: `BandwidthInfo` in `src/ai_api/types.rs` and `NetworkState` in
`src/backend.rs`. Both had escaped `e425908` for the same reason — nothing
constructs them, so nothing had made me look.

`NetworkState` turned out to be the interesting one. It is a public type, and
`FullSystemState::network` is a public field serialised by `simon cli all
--format json`. `get_full_system_state` builds every other section and never
assigns that one. Asked the binary:

```
simon cli all --format json:
  cpu             present
  memory          present
  accelerators    3
  disks           4
  network         0        <- on a machine with 20 interfaces
  top_processes   10
  system          present
```

An empty array is a worse answer than a missing key, because it reads as a
finding: this host has no network interfaces. It is served by a process that had
just enumerated twenty of them for its own `network` subcommand.

The fix populates the field, and takes the rate fields to `Option<f64>` on the
way through — they come back `None` here by construction, since the enumeration
happens through a fresh `NetworkMonitor` (baselines need `&mut`, the method takes
`&self`) and a rate needs two samples. `None` on a one-shot dump is the honest
answer; `0.0` would have been the same fabrication `e425908` removed everywhere
else. A caller that wants throughput holds a monitor and samples it twice.

Verified: 20 entries with byte counters and null rates, where the field was `[]`.

**A field nobody constructs is a field nobody has audited.** Both survivors of
the network-rate sweep were unconstructed types. Grepping for a *pattern* finds
the code that runs; the code that does not run keeps whatever shape it was born
with, and stays wrong quietly until someone wires it up. Worth checking the
public types no constructor mentions — the ones exported through an API are the
ones a consumer will eventually hit.

### Disk throughput, zero on every platform, drawn on three surfaces

Went to close the last three dashboard gaps at their cause — the pipeline
`Snapshot` not carrying the readings — and found the reading it *does* carry is
a constant.

```rust
// Windows
read_rate: 0.0,
write_rate: 0.0,
// everywhere else
io.read_throughput.unwrap_or(0) as f64,
```

Both are zero. Windows takes the cheap `logical_drives` path, which reads
capacity and no I/O counters at all, and hardcodes it. The other platforms flatten
`read_throughput`, which is itself **always `None`** — a rate needs two samples
to difference and `DiskIoStats` is built from one, which the entry about
`disk.{n}.read_rate` established some fifteen commits ago.

So `DiskSnapshot::read_rate` has been `0.0` on every platform for the life of
the pipeline, and it is drawn by the TUI, the GUI, and — since two commits ago —
`simon_disk_read_bytes_per_sec`. **I renamed and labelled that metric earlier
today, which made a constant zero more accurately addressed.** Correcting a
name without checking the value behind it is its own small lesson.

Both fields are `Option<f64>` now, matching `NetSnapshot` after the equivalent
fix: `None` on the Windows capacity-only path, and the real `Option` from
`io_stats` elsewhere. Six consumers followed — the metric, the TUI row, the TUI
totals, the GUI, a placeholder row and the plausibility test — and every one of
them was flattening an absence to zero at its own layer.

**Worth keeping.** Three separate rate defects this session had the same shape:
network bandwidth differenced against a baseline overwritten a line earlier,
disk throughput hardcoded, and `read_throughput` never computed at all. **A rate
is the most fragile thing this crate publishes** — it needs two readings, a
clock, and somewhere to keep the first — and every place one is stored is worth
checking against the possibility that nobody ever computed it.

### Checking my own open-work item, which was already stale

The open-work entry opened by `d4da86c` says the Prometheus endpoint serves
**10 of the 24** metrics the dashboards query. That was true when written. Then
`5796371` moved the recorder to labels, which was done for a different reason —
`simon_gpu_0_utilization_percent` could never match
`simon_gpu_utilization_percent{gpu="0"}` — and in doing so it made most of those
names matchable.

Measured rather than assumed: **20 of 24**. The item had been wrong for two
commits, and I wrote both of them.

`simon_uptime_seconds` turned out to be one line: `SystemStats::uptime_seconds`
sits in the same struct the recorder was already reading for the load average.
**21 of 24.**

The last three are genuinely blocked by what a `Snapshot` carries:

* `simon_cpu_temperature_celsius` — no CPU temperature on `CpuStats`; the
  exporter reads `hwmon::read_cpu_temperatures` directly, which the recorder
  cannot do without becoming impure and untestable again.
* `simon_disk_read_bytes_total` / `write_bytes_total` — `DiskSnapshot` carries
  `read_rate` and `write_rate` and no cumulative counter. Publishing a rate
  under a `_total` name is what `5796371` removed; adding the counters means
  adding them to the pipeline snapshot.

A coverage test now records this in the code, and asserts on the names a
synthetic snapshot genuinely carries data for.

**Worth keeping: an open-work item is a claim, and claims go stale.** This one
was measured once, written down, and then invalidated by the next commit but
one — by me, without noticing, because I was looking at labels rather than at
coverage. **Re-measuring before quoting a number costs one command; quoting a
stale one sends the next person to fix something already fixed.**

### A contract that was right for a loop and wrong for everything else

Having just fixed the CLI's USB output, the same question about the line above
it. `simon cli cpu` said:

```
Clock: not read — Windows reports the nominal clock, not the current one
```

on a machine whose cores were sitting at 5 GHz, hours after the per-core clock
was implemented and measured at 4732–5322 MHz.

**The cause was the contract I designed for it.** The PDH reader primed on the
first call and returned `None` so that the *second* would have an interval to
difference against. That is correct for a monitoring loop — and wrong for
everything else, because **a one-shot process only ever makes a first call.**
The CLI, one ontology snapshot, one agent tool call: all of them got the priming
return, forever. I wrote "the first call after opening returns nothing" as
though it were a caveat, and it was a defect for every caller that is not a loop.

The interval is now primed when the query opens — one collection, a 120 ms
sleep, and every call from the first onward works. It costs 120 ms once per
process, on the first read of a CPU frequency and never again.

```
before: Clock: not read — Windows reports the nominal clock, not the current one
after:  Clock: 5265 MHz (max 4400 MHz)
```

**Worth keeping.** The measurement was right, the reader was right, and the
*shape of the API* threw the answer away for most callers. A contract that says
"call me twice" is a defect wearing documentation, and it was invisible in the
place I verified it — a loop — which is exactly where it works.

**And a machine note.** `clippy-driver` crashed with `STATUS_STACK_BUFFER_OVERRUN`
during this entry, which reads exactly like a compiler bug. The disk was at
**2.0 GB free**. It is the third time this session that a tooling failure with a
confident-looking error message was really this volume being full.

### The last unaudited surface, and a fix that never reached the screen

Running every `simon cli` subcommand and reading the output — the one surface
this session had not driven. **It is in good shape**, and the entries in this
file are visible in it:

```
Clock: not read — Windows reports the nominal clock, not the current one (max 4400 MHz)
disk.0.temperature   this device exposes no thermal sensor
Master Volume: not read — simon has no mixer binding on this platform
```

Two defects, both in presentation rather than reading.

**`simon cli usb` printed `(Unknown)` for every device** — and that was the
*speed*, rendered as an enum variant name. `UsbSpeed::Unknown` means the
negotiated speed is not read on Windows, which the entry four below documents at
length; printed as `Unknown` it looks like a property of the device. It now says
`speed not read`.

**And the class was not printed at all.** `e456c53` taught the reader to get a
real class for 24 of this machine's 39 USB devices, from the descriptor and the
hub identifiers — and the CLI, the surface a person actually looks at, went on
showing nothing:

```
before: [046d:c548] Logitech USB Input Device (Unknown)
after:  [046d:c548] Logitech USB Input Device (Hid, speed not read)
        [17ef:4839] Lenovo 510 IR Camera        (Video, speed not read)
```

`simon cli audio` printed `- Some(Active)`: `{:?}` on an `Option`, showing the
consumer the wrapper. Now `Active`, or `state not read`.

**Worth keeping: a reader fix is not finished when the reader is fixed.** The
USB class work went into the ontology, the agent tools and the JSON surface, and
stopped one layer short of the screen — because those were the surfaces being
audited that day. **The question "who displays this?" belongs beside "what else
stores this?"**, and both are cheap greps that this session has now been caught
skipping twice.

### Two auth flags that decide nothing, and the probe that found them

Auditing the REST surface the way the tool catalogue was audited: drive it
directly and read what comes back. It refused everything.

```rust
let cfg = ApiConfig { require_auth: false, allow_anonymous_read: true, ..Default::default() };
// -> memory: ERR Permission denied: Invalid API key
```

Both flags are **inert**. `ObservabilityApi::new` keeps only `config.keys`, and
every method looks its caller up in that map, so `require_auth = false` does not
disable authentication.

**The behaviour is correct and was already understood.** `ServerConfig` carries
its own `allow_anonymous`, `http_server` sets it as
`config.api_key.is_none()`, and a comment on that field says outright that
`ObservabilityApi::new` "drops `require_auth` / `allow_anonymous_read`. So the
flag has to live here to reach the gate at all." The gate is the right place —
it is what sees a request carrying no key.

What was wrong is that `http_server` still assigns both dead fields in both
branches, forty lines from the comment explaining they are dropped. Read on its
own, that block is a configuration of access control; it is a no-op. The fields
now say **"Not consulted"** in their own doc comments, which is where someone
setting them will look, and the assignments say what actually decides the
question.

**Worth keeping: the direction of the failure.** These flags fail *closed* — a
`require_auth = false` that keeps requiring auth denies access it was told to
permit, which is the safe way round and is why nobody noticed. A flag that fails
closed is invisible in production and still a lie in the source, and the next
person to add a flag beside it will assume the neighbours work.

### The sentinel put back at the point of use

`SwapInfo::used` became `Option` in `15a60ab` so that an unread pagefile stays
distinguishable from an empty one. Then:

```rust
collector.record("simon_swap_used_bytes", mem.swap.used_or_zero() as f64 * 1024.0);
```

**The absence is reconstructed as a zero at the point where a machine reads
it.** `simon_swap_used_bytes 0` is the sentence "nothing is paged out", and the
JSON tools said the same in four more places. Fixing a type does not fix the
call sites; it only makes them say what they are doing.

There are 49 `_or_zero()` calls, and **most are correct** — the helper's own doc
says it "exists so that choosing not to is visible at the call site", and the
CLI display paths guard with `if total_or_zero() > 0`, which reads "was swap
reported and non-empty". `main.rs` even carries a comment noting the guard. So
this was not a sweep: it was the handful where an unguarded sentinel reaches a
machine.

* the Prometheus gauge — now emitted only when the pagefile was read
* `tool_get_memory_status` and `tool_get_swap_status`, on both the Linux and
  Windows branches — now `null`, matching the macOS branch fixed in `8ed7e71`
* `SystemSummary::swap_total_mb` / `swap_used_mb` — now `Option<u64>`

Verified on this host: swap total 51186 MB, used 1792 MB, `cached_kb: null`. The
totals match the real pagefile, and the one quantity Windows does not report
says so.

**Worth keeping.** Every one of these was written *after* the `Option` existed,
by someone reaching for the helper that makes the compiler stop complaining.
**A type that can express absence only helps where callers are willing to carry
it**, and `_or_zero()` is precisely the shape of an escape hatch that gets used
by reflex. The doc comment anticipating that is why the guarded uses are fine
and these five were not.

**A machine note.** The disk hit **literally zero bytes free** during this
entry, which crashed `rustc` with no diagnostic — an exit code and an empty
error, which reads exactly like a compiler bug. `cargo clean -p` recovered it.
`target/` is 20 GB of a 3.7 TB drive, so this is not the build cache's doing;
it is worth someone looking at what is filling that volume.

### Doing the thing the previous commit deferred

The entry below ends: *"Factoring that loop body into a function taking a
`Snapshot` would make it testable, and is worth doing by whoever next touches
it."* I was the one next touching it, and deferring a two-hour job to an
imaginary successor is how the defects in that same entry survived as long as
they did.

`metric_collection_loop` is now four lines around
`HttpServer::record_snapshot(&collector, &snapshots.latest())`, and
`record_snapshot` takes a plain `&Snapshot`, which `Default` constructs. The
loop still needs a live pipeline and a tokio runtime; the part that decides
metric names and labels no longer does.

Two tests came straight out of that:

* a snapshot with one disk must render
  `simon_disk_read_bytes_per_sec{device="PhysicalDrive0"}` — labelled, and named
  as the rate it is — and must contain neither `simon_disk_0_` nor anything
  claiming to be a `_total`, since `DiskSnapshot` carries no cumulative counter.
* an empty snapshot records nothing rather than zeros.

Both were confirmed by restoring the old `simon_disk_0_read_bytes_total` form
and watching the first fail.

**What this really bought.** The two defects in the entry below — the instance
in the name, the rate called a total — were not subtle, and they survived for
the life of the endpoint. Not because they were hard to see, but because
**nothing could look at them without standing up a server**. The code was
correct-looking in review and unobservable in a test, and that combination is
where this kind of thing lives. Moving eighty lines out of an `async fn` changed
nothing about behaviour and changed everything about whether the next mistake
gets caught.

### The instance in the metric name, and a rate called a total

The entry below said the server's collection loop recorded only whole-machine
metrics because `MetricCollector` mangled labels. Having fixed the mangling, the
loop turned out to record GPU and disk metrics after all — just unusably.

```rust
let g = |name: &str| format!("simon_gpu_{i}_{name}");
collector.record(&g("utilization_percent"), ...);
collector.record(&format!("simon_disk_{i}_read_bytes_total"), disk.read_rate);
```

**The instance was baked into the metric name.** Every bundled dashboard queries
`simon_gpu_utilization_percent{gpu="0"}`, so the panels could never match
whatever the values were. It also defeats aggregation outright: `sum by (gpu)`
has nothing to group on when each card is a different metric name.

And `simon_disk_{i}_read_bytes_total` was fed `disk.read_rate`. **A rate under a
`_total` name** — `DiskSnapshot` carries `read_rate` and `write_rate` and no
cumulative counter at all, so `rate()` over that series in a dashboard computes
the rate of a rate. That is the third appearance of this exact confusion in this
session, after `DiskReadBytesPerSec` stored in a field documented as a total and
the commit-charge-as-swap reading. It is now
`simon_disk_read_bytes_per_sec{device="..."}`, which is what the number is.

Both are labelled now. The cumulative disk totals the dashboards want exist only
on the `PrometheusExporter` path, which reads `io_stats` directly — one more
reason the endpoint should serve that exporter, which is the open-work item the
entry below opened.

**A limit worth stating.** This was verified by compiling, by grepping that no
metric name still carries an index, and by the label round-trip test on the
renderer. It was **not** verified by scraping a running server: the recording
lives inline in an async loop that needs a live pipeline, and I did not stand one
up. Factoring that loop body into a function taking a `Snapshot` would make it
testable, and is worth doing by whoever next touches it.

### Two Prometheus renderers, and the server serves the worse one

Following the dashboard contract to the endpoint that actually answers a scrape.
`/api/v1/metrics/prometheus` does **not** serve `PrometheusExporter` — the
complete implementation the entries below fixed and tested. It serves
`MetricCollector::export_prometheus`, a second renderer in
`observability/metrics.rs`, and **nothing outside `src/prometheus.rs` references
the first one at all.** The good exporter is unreachable and the served one is
the smaller.

The gap is not subtle. The exporter knows 30 metric names; the server's
collection loop records 10, and the dashboards query 24.

The served renderer also emitted its own storage key. `record_with_labels`
encodes a labelled series as `name:{gpu=0,vendor=NVIDIA}` so each label set gets
its own series — a map key, not exposition syntax — and `export_prometheus`
printed it verbatim:

```
simon_gpu_temperature_celsius:{gpu=1,vendor=NVIDIA} 37
```

Prometheus wants `name{gpu="1",vendor="NVIDIA"}`: quoted values, no colon. That
is fixed, with the inverse of the encoder sitting next to it, and a test that
was confirmed to fail against the old renderer.

**Nothing has served a malformed line yet**, because the collection loop only
calls the unlabelled `record` — and that is precisely why the loop is limited to
whole-machine metrics, which is why fourteen dashboard panels have nothing to
draw. The label encoding was the blocker, and it is gone.

**What is deliberately not done.** Wiring the endpoint to the full exporter is
the obvious next step and is not a one-line change: a synchronous
`collect_system_metrics()` costs **1.4–2.6 seconds** on this machine, 870 ms of
it the profile inspector walking 23,000 driver settings, which is far too slow
inside a scrape handler. The right shape is the existing background loop — which
already reads a concurrently-collected pipeline snapshot rather than
re-enumerating hardware — recording the per-instance metrics through
`record_with_labels` now that it renders correctly. That is a change to an async
server I cannot exercise on this machine, so it is recorded rather than guessed
at.

### Dashboards querying names nothing published, and three wrong tests

`grafana/` ships three dashboards, and they are a contract nobody was checking.
Comparing the 24 metric names they query against what the two publishers emit:
**six were published by neither**, so those panels render empty against a live
server — which looks like broken hardware rather than a broken dashboard.
`http_server.rs` already documents this exact class happening once before, when
the names lacked the `simon_` prefix and all three dashboards were blank.

* `simon_gpu_clock_graphics_mhz` — the exporter published the same number as
  `simon_gpu_clock_core_mhz`. NVML calls it the graphics clock and the ontology
  publishes `gpu.{n}.clocks.graphics`, so `core` was the one name in the crate
  that matched nothing. Renamed.
* `simon_swap_used_bytes`, `simon_uptime_seconds`, `simon_cpu_temperature_celsius`,
  `simon_disk_read_bytes_total`, `simon_disk_write_bytes_total` — all quantities
  the crate already collects and none of them published. Added.

**The guard took three attempts and each failure is the useful part.**

1. **Searched the publishers' source text.** Passed while the defect was
   restored, because the comment explaining the defect contained the name it
   was looking for. **A test a comment can satisfy is not a test.**
2. **Searched the rendered output.** Now flagged
   `simon_cpu_temperature_celsius` on a machine whose CPU exposes no readable
   sensor. Output cannot distinguish "the exporter does not know this name"
   from "it knows it and had nothing to report", and only the first is a defect.
3. **Searched the source with comments stripped.** That is the question actually
   worth asking — does the code contain a publisher for this name — and neither
   prose nor an absent sensor can answer it. Verified by restoring the `core`
   rename and watching it fail.

**A test was also deleted.** The per-instance-label check from the entry below
demanded an exemption for every legitimately whole-machine metric:
`simon_gpu_count`, then `simon_swap_used_bytes`, then `simon_uptime_seconds` —
three in one sitting, none a defect. **A test that needs a growing allowlist to
stay green has stopped testing an invariant and started describing the current
output.** The failure it existed to catch produces duplicate samples, and
`no_two_samples_share_a_name_and_labels` catches that exactly, with no heuristic.

### A metrics endpoint Prometheus would reject in full

The tool catalogue turned out to be productive because it was a machine-consumed
surface with no conformance tests. The Prometheus exporter is the other one:
`tests/` had no file for it, and the network-rate defect three entries below had
been exporting a hard zero from it for the life of the exporter. So the same
treatment — render the output and read it.

**`simon_network_rx_bytes_total` was emitted twenty times, unlabelled, with
twenty different values.**

```
simon_network_rx_bytes_total 43054075
# HELP simon_network_rx_bytes_total Total bytes received
# TYPE simon_network_rx_bytes_total counter
simon_network_rx_bytes_total 10988371
...
```

`collect_network_metrics` builds an `interface` label and then calls
`MetricFamily::counter`, which takes no labels and drops it. This is not a wrong
number on a dashboard. **Prometheus rejects a scrape containing duplicate
samples**, so the network section discards every other metric the endpoint
serves — CPU, memory, GPU, all of it — and the failure appears as an empty
target rather than as a bad value.

The repeated `# HELP` lines are a second, independent defect with the same
reach. `PrometheusExporter::add` pushed a *new family* per sample, so the
headers repeated once per instance: 6× for disks, 3× for GPUs, 20× for network.
`MetricFamily` already held a `Vec<MetricSample>`, so the structure was right
and only the insertion was wrong — `add` merges by name now, which fixes every
family at once.

And `network_tx_bytes_total` was never exported at all. The receive counter had
no counterpart, so a dashboard could plot half of every link.

`tests/prometheus_exposition.rs` now holds three guards — one HELP and TYPE per
name, no duplicate name+label pair, and every per-instance metric carrying a
label. All three were confirmed by restoring the defects and watching them fail,
then confirming the restore.

**One of them found a false positive in itself, which is worth recording.** The
label guard flagged `simon_gpu_count 3`. That metric is correct: a count of GPUs
is a fact about the machine, not about any GPU, so it rightly carries no `gpu`
label. The rule now exempts `_count` **on that reasoning** rather than by
widening a prefix match until the failure disappears — the two look identical in
the diff and only one of them still tests anything.

### The two pairings the guard did not cover, and why one is separate

The round-trip guard below shipped with four pairings. The catalogue has six
list-to-details relationships, so the two it missed were worth adding rather
than leaving for the next sweep to rediscover by hand.

**GPU**, added to the table. It exposed an assumption in the guard itself: the
first version read ids with `Value::as_str`, and `get_gpu_list` numbers its rows.
So the id extraction now accepts a string *or* a number and passes it through
unchanged, because the details tool parses whatever it is handed. Also worth
knowing when reading the table: the listing calls the field `index` and the
details tool wants `gpu_index`.

**Processes, deliberately not in the table.** A pid is the one identifier that
can stop being valid between the call that hands it out and the call that uses
it. A table-driven check over every listed process would fail whenever one
exited — and a flaky test teaches people to ignore it, which is worse than not
having it. `the_process_details_tool_resolves_a_live_pid` asserts the same
contract against the one pid guaranteed to still exist: the test's own.

**Both were confirmed by breaking them.** Renaming the GPU listing's `index`
field produced

```
"get_gpu_list returned a row with no usable `index`, so nothing can be asked
 about it: {"index_BROKEN":0,"name":"NVIDIA GeForce RTX 3090 Ti",...}"
```

and the restore was checked by grepping the sabotage back to zero occurrences
before gating. That is now twice in two commits that this step earned its
keep — the first time it revealed the test was watching nothing, this time it
confirmed the extension works on a numeric id path the original could not have
handled.

### A guard for the regression, and nearly a test that proved nothing

`tests/ai_tool_surface.rs` already guards two things the sweep would otherwise
have to find by hand: absence *words* like `"Unknown"` in tool output, and tools
advertised in the catalogue with no dispatch arm. It did not guard the contract
the regression two entries below broke — **an identifier a listing hands out
must work in the matching details tool**. That is the thing an agent relies on
without being told: call the list, take an id from a row, ask for detail on it.

`an_id_from_a_listing_resolves_in_its_details_tool` now checks four pairings —
USB, displays, disks, network interfaces — and fails with the specific pairing
named. A listing that is empty on the machine running the test is skipped rather
than failed, because no USB devices is a legitimate state for a container and
this is a test about self-consistency, not about hardware.

**The part worth recording is that it nearly did not test anything.** Written,
run, green — and green proves nothing on its own, so the next step was to break
the code deliberately and watch it fail. It did not fail. The "regression" I had
introduced to challenge it had landed in `get_bluetooth_devices`, because that
is the first function in the file matching the pattern I edited, and the USB
listing was untouched. **A green test against unmodified code.**

Breaking the right function produced what was wanted:

```
an_id_from_a_listing_resolves_in_its_details_tool --- FAILED
  "get_usb_devices advertised address=\"BROKEN_usb_root_hub30_9_eba7ce_0_0\",
   and get_usb_device_details cannot resolve it"
```

Two entries below, the finding was a test that asserted a fabricated zero and so
converted a defect into a requirement. This is the same lesson approached from
the other side: **a test you have never seen fail is not yet evidence, and
confirming it fails means confirming you broke the thing it watches.** I checked
the first, nearly skipped the second, and the second was where the mistake was.

### The zero left behind when a field stopped being the identity

Following the entry below to its end. The regression there was that
`get_usb_device_details` keyed on `bus_number` and `port_number` after the
Windows reader stopped filling them. Fixing the lookup left the cause in place:
**two public fields holding `0` on Windows, meaning "not reported"** — the exact
sentinel this session exists to remove, sitting in the type that the sweep had
just proved consumers trust.

Both are `Option<u8>` now. Windows reports neither, so both are `None`; Linux
parses them out of the sysfs name and macOS out of the `Location ID`, so both
report real ones there.

The compiler then found the consumer I would not have looked for: the shipped
`examples/usb_monitor.rs` prints

```
  Bus/Port: 0/0
```

for every device on Windows, and had done since the reader was written. It now
prints the address — the key that is always present and is what the details tool
wants — with a dash for each of bus and port where the platform gives none.

**Worth keeping: a lookup and a display are the same defect wearing different
clothes.** The lookup collapsed 39 devices into one and was found by a sweep;
the display showed `0/0` to every user of that example and would never have
failed anything. Changing the type caught both, because a type is the only one
of the three — comment, test, type — that every consumer has to answer to.

### A regression I introduced, found by the sweep that follows the fix

The tool sweep had only called each tool with `{}`, so the eight that need
arguments returned "X is required" before any reader ran. Calling them with real
values — a drive name, a pid, an interface, a search pattern, both a valid and a
deliberately invalid one each — found the untested half behaving well.

Except one, and it was mine.

`get_usb_device_details` finds a device by `bus_number == bus && port_number ==
address`. When the id scheme moved to the platform device path, the Windows
reader stopped filling those fields — they are `0` and `0` for every device it
enumerates. So:

```
39 devices -> 1 distinct (bus, port) pair
```

**The tool could only ever return whichever device came first, and every other
USB device on the machine became unreachable through it.** `get_usb_devices` was
also still advertising `bus` and `port`, so an agent following the catalogue
would ask a question that cannot be answered.

Both now use `address`, the same key the ontology uses for `usb.{addr}`, and the
tool definition says so with a worked example. **39 of 39 advertised addresses
resolve**, which is the property that was silently false.

**What makes this worth its own entry: I wrote the rule and then broke it.** Two
commits earlier this file says, in as many words, "the question *what else
stores this?* is worth asking after every reader fix, not only after the ones
that look structural". I changed what identifies a USB device and did not ask it
of `bus_number`. One grep would have found the call site.

The sweep that caught it is the same one that found the original defects, run
again after the change. **A verification pass is not finished when the fix
lands; the fix is a change like any other and deserves the pass that found the
bug.**

Two things checked and cleared while here: displays round-trip correctly
(`get_display_list` hands out `\.\DISPLAY1` and `get_display_details` accepts
it — my first probe passed an integer, which is my error and not the code's),
and `search_processes` with no matches returns `match_count: 0` with
`success: true`, which is right: a search that found nothing succeeded.
`apply_profile_setting` was deliberately not exercised, because it changes real
system settings.

### A 16 MB drive reported as sizeless, by integer division

The smallest finding of the tool-surface sweep, kept because the failure mode is
one the rest of this file does not cover. `get_disk_list`:

```json
{"model":"Linux File-Stor Gadget USB Device","size_gb":0}
```

Nothing fabricated a zero here. The drive has a real, correctly read capacity —
the ontology reports `disk.0.capacity = 16450560`, which is 15.7 MB — and

```rust
"size_gb": info.capacity / 1024 / 1024 / 1024,
```

is integer division, so **every drive smaller than a gibibyte reports as
sizeless**. The reader was right, the arithmetic was right, and the unit chosen
for the answer destroyed it.

Both call sites now emit the exact byte count alongside a float:

```
PhysicalDrive3  size_gb=0.015     size_bytes=16450560
PhysicalDrive1  size_gb=3726.021  size_bytes=4000784417280
```

**Worth keeping: the other entries in this file are about readers that did not
read. This one is about a value that survived the reader and died in the
presentation.** A unit coarse enough to round a real measurement to zero
produces the same sentence an agent has been taught to distrust — and it will
pass every check in this crate, because nothing here inspects the arithmetic
between a correct reading and the JSON.

### Two fabricated zeros next to the comment explaining why zero is wrong

`get_memory_breakdown` answered `buffers_kb: 0, cached_kb: 0, shared_kb: null`
— three sibling fields, one of them honest. The reader:

```rust
buffers: 0, // Windows doesn't expose this separately
cached: 0,  // Could use GetPerformanceInfo for SystemCache
// Not read on Windows. `None` rather than a zero that reads like a
// measurement of no shared memory.
shared: None,
```

**`shared` was fixed in 6.0.0 with a comment stating exactly why a zero is
wrong, and the two lines immediately above it kept theirs** — under comments
that admit, in as many words, that neither number was measured. This is the
"fix applied where the defect was seen, not where it is" pattern in its most
literal possible form: adjacent lines, same struct, same commit's blast radius,
and the reasoning already written down one line below.

Both are `Option` now, and the two answers differ:

* **`buffers` is `None` on Windows and macOS.** "Buffers" is a Linux
  `/proc/meminfo` line; neither other platform has the concept, so there is no
  figure rather than a figure of zero.
* **`cached` is now read on Windows**, from `GetPerformanceInfo`'s
  `SystemCache`. The comment was right that it was available; nobody had done
  it. **52.1 GB on this host, where the field previously said 0.**

**And a trap, checked before trusting the number.** `SystemCache` reads 52.1 GB
while perfmon's `\Memory\Cache Bytes` reads **1.0 GB** on the same machine at
the same moment. They are different quantities: the first counts every cached
page (standby 48.5 GB + modified 0.4 GB + resident 1.0 GB, which is Task
Manager's "Cached"), the second counts only the cache's own resident working
set. The 52 GB figure is the one that corresponds to Linux's `Cached` line,
which is what the field means. That is now in a comment saying *do not "correct"
this to `Cache Bytes`* — because a fifty-fold discrepancy against a
plausible-looking counter is exactly the kind of thing a future reader fixes in
the wrong direction.

The observability layer had been converting the sentinel back with
`if stats.ram.cached > 0 { Some(..) } else { None }`, which also erased a
genuine zero. That reconstruction is gone; the absence is carried in the type.

### Every network rate in the crate was zero, on every platform

Found by calling all 52 agent tools and reading the JSON. `get_network_bandwidth`
returned `rx_bytes_per_sec: 0.0` for all eleven interfaces — and again on the
second call, and the third, on a machine that was pushing to git throughout.

```rust
pub fn interfaces(&mut self) -> Result<Vec<NetworkInterfaceInfo>> {
    let interfaces = Self::enumerate_interfaces()?;
    self.update_prev_stats(&interfaces);   // <- baseline := the values
    Ok(interfaces)                         //    about to be returned
}
```

Every caller fetches interfaces and then asks for a rate. Fetching **overwrites
the baseline with the values it is about to hand you**, so `bandwidth_rate`
computes `current - current`. The answer is exactly zero, always, on every
platform, and it looks like a quiet network rather than a broken subtraction.

`bandwidth_rate` also returned `(0.0, 0.0)` when it had no baseline at all, so
even the ordering fix alone would have kept a fabricated zero on the first
sample. It returns `Option` now: the first call for an interface records a
sample and reports nothing, the same contract as the PDH per-core clock reader
and for the same reason.

**Seven call sites, and the reach is the point:** the agent tools, the
observability API, the Prometheus exporter, the pipeline that feeds both
frontends, the TUI, the GUI, and the CSV export. `network_rx_bytes_per_sec` has
been exported to Prometheus as a hard zero for the life of that exporter.
Each site now says absence in its own idiom — `null` in JSON, an omitted series
in Prometheus (an absent series already means "not reported"), an em dash in
both frontends, an empty CSV field, and a skipped sample in the history graphs
rather than a trough that never happened.

**`DB_VERSION` goes to 5.** `SystemSnapshot::net_rx_bps` was a `u64` fed from
this reader, so every recorded row holds a zero that means "not measured" — the
distinction the per-process `net_rx_bps` beside it already preserved as
`Option<u64>`. That is the second version bump today and the reason is the same
as the first: the layout still parses, which is exactly why a reader cannot be
allowed to mix the two definitions silently.

Before and after, same machine, four consecutive calls:

```
before: 11 interfaces, 11 reporting exactly 0.0, forever
after:  call 1 -> 11 null (priming)
        calls 2-4 -> 2 interfaces with real traffic, 9 genuinely idle
```

**A test pinned the defect in place.** `test_bandwidth_rate_no_prev` asserted
`rx == 0.0 && tx == 0.0` — it existed to confirm the fabrication, and it passed
for the entire life of the bug. It now asserts `None`, and a second test checks
that a real second sample produces a real rate. **A test that encodes the wrong
answer is worse than no test: it converts a defect into a requirement.**

### Zero hertz and zero swap, handed to an agent as facts

Two audits in a row came back clean, so this followed the pointer the fallback
entry left: **triage `unwrap_or` by blast radius, and the agent tool surface is
the largest radius there is** — a `0` in a TUI gauge is cosmetic, a `0` handed
to an LLM becomes a premise it reasons from. Fifteen such fallbacks in
`ai_api/`. Two are real defects, both on macOS, both the exact shape this
session has been fixing on Windows.

**`tool_get_cpu_cores`** read `hw.cpufrequency` and did `unwrap_or(0)`:

```rust
"frequency_mhz": freq_hz / 1_000_000,
```

`hw.cpufrequency` **does not exist on Apple Silicon** — Apple removed it with
the ARM transition — so this reports `"frequency_mhz": 0` for every core of
every M-series Mac. It is two defects at once: a fabricated zero, and *one
figure broadcast across every core*, which is precisely what `24a7314` fixed in
the macOS silicon reader and `f5a54ee` fixed on Windows. The same commit also
replaced a missing model string with the literal `"CPU"` — a placeholder name an
agent will repeat back as though it were a part number. Both are `null` now.

**`tool_get_swap_info`** ran `sysctl -n vm.swapusage` into `unwrap_or_default()`,
so a failed command became an empty string that both parses read as `0`:

```
"total_kb": 0, "used_kb": 0, "cached_kb": 0, "usage_percent": 0.0
```

An agent asked "is this machine swapping" is told there is no swap configured,
when what happened is that the command did not run. **This is `15a60ab`
mirrored**: there, Windows commit charge was published as swap and read 97 GB
too high; here, a failed read is published as swap and reads all the way low.
`cached_kb` was a hardcoded `0` for a quantity never read at all — the same
thing the Windows reader says `None` to, with a comment.

**Stated plainly: none of this is verified on hardware.** There is no Mac here.
The defects are certain by inspection — `unwrap_or(0)` on a `sysctl` that does
not exist on the current architecture is not a judgement call — but the
corrected code has been compiled for `aarch64-apple-darwin` and not run. That
is the same standing caveat as every other macOS change in this session, and it
is why the open-work item about macOS being unexercised stays open.

**Worth keeping: the audits that found nothing still directed this.** Two clean
sweeps meant the ontology's own surface was in good order, which is what made it
worth walking to a *different* surface. The tool catalogue is not covered by the
ontology conformance tests at all — no entity declares it, no `push_opt` guards
it, and the honesty rules this crate enforces on `snapshot()` simply do not
reach it.

### Every measurement, checked for whether it actually moves

**The audit that would have caught this session's biggest defect, run
deliberately.** A `Measurement`-kind reading that never changes is a candidate
for a specification wearing a measurement's provenance — which is exactly what
`CurrentMhz` was, reading 4400 on all 24 cores idle and loaded, and what the
removed `GetSystemTimes` reader was, reading 100% always.

Method: snapshot, saturate the CPU, snapshot again, and list every
`Measurement` entity whose value is byte-identical across the two. 79 moved, 97
did not.

**Nothing was wrong.** The 97 are all legitimately slow: SMART counters and
power-on hours, drive temperatures on their own update cadence, service counts,
and every GPU figure — because the load generator only stressed CPU, so an idle
GPU sitting at 210 MHz is the correct reading, not a frozen one. **Filtering to
the domains the load actually touches left zero frozen `cpu.*` measurements.**

The specific confirmation is worth having, because it is the field this
session rewrote:

```
cpu.core.0.frequency   idle=None            loaded=5027 MHz
cpu.core.1.frequency   idle=None            loaded=5072 MHz
```

`None` on the priming call, real boosting clocks after — the PDH contract
behaving as designed, on the field that used to be a constant 4400.

**One scare, which was my own test.** `cpu.total.utilization` read 17.74% idle
and 17.73% under full load, which looks exactly like a dead reader. It is not.
The interval is *time since the previous call*, so two back-to-back snapshots
make the second one span milliseconds. Spacing the calls out:

```
idle:        17.85%  then 6.73%   (back-to-back, second window tiny)
half loaded: 55.70  55.92  56.17  56.00
independent per-core mean:        55.69
```

Twelve of twenty-four threads busy, plus background, is 56%. It is correct to
two decimal places against a separate reader.

That behaviour is now in the entity description with the numbers, because an
agent polling twice in a row gets a real measurement of a meaningless window and
nothing previously said so.

**Worth keeping: a negative result from a good technique is still worth the
run.** Three audits in a row found defects; this one found none, and that is
evidence about the crate rather than about the method. The method is also cheap
to repeat — it is thirty lines and it directly tests the property this whole
codebase exists to protect.

### The last misleading absence, and a deliberate refusal to implement

The fourth and last suspicious absence reason from the audit two entries below:
`usb.{addr}.speed`, absent on all 39 devices, blamed on *"the platform did not
report a negotiated bus speed"*.

Checked it properly. Windows exposes no speed among a USB device's PnP
properties — `Get-PnpDeviceProperty` returns `Address`,
`ReportedDeviceIdsHash`, `BusReportedDeviceDesc` and nothing about speed. So the
reason is not false in the way `usb.{addr}.class` was. **But it is still
misleading**, because the value is obtainable: it comes from
`IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX` issued against the *parent hub*,
addressed by the port number in the device's `LocationInformation`. The reason
now says exactly that, ending "it is obtainable and unimplemented, not
unavailable".

**I did not implement it, and the reasoning is the point of this entry.** It
needs unsafe FFI: opening hub handles, a device-to-parent-hub traversal, an
IOCTL, and a struct parse. Only 14 of this machine's 39 devices carry the
`Port_#.Hub_#` location that addresses the call. And there is no ground truth
available to check the result against — Windows reports negotiated speed nowhere
else, which is the entire reason the IOCTL exists — so the only available
verification is *"do these numbers look plausible"*.

**That is the exact standard this session has spent thirty commits removing
from this crate.** A name heuristic that classified USB devices, a nominal clock
published as a current one, commit charge published as swap: every one of them
looked plausible. Writing unsafe code whose correctness I could only assess by
squinting at it would be the same mistake in a new place, and shipping it would
be worse than the honest absence that is there now.

So the field stays absent, the reason now names the API, the addressing scheme
and the obstacle, and the next person starts from there rather than from the
symptom. **`2276c9c` exists because `f5a54ee` wrote a note like this instead of
guessing — a deferral that records what the next person needs is worth more than
an implementation nobody can check.**

### A class guessed from the device's name, when it declares one

The audit below said the next thing to check was every absence *reason*, not
just the ones phrased as absolutes. There are 40 distinct reasons in a snapshot.
Most are specific and true. Four attribute to "the platform" what might be "we
do not read it", and they were worth testing one by one:

* `pci.{addr}.numa_node`, absent on all 64 devices — **accurate**. Windows
  exposes no NUMA property on these devices at all.
* `usb.{addr}.class`, absent on 22 of 39, reason *"the device class descriptor
  was not readable"* — **false**, and the trail led somewhere better.

The class was not being read from a descriptor at all. It came from
`classify_usb_device(name, description)`, which matched substrings of the
device's *display name*: `"hub"` meant Hub, `"disk"` meant MassStorage,
`"camera"` meant Video. **That is the same shape as the USB speed heuristic
removed earlier in this file** — what a device is *called* standing in for what
it *declares* — and it fails in both directions without saying so.

Windows records the real thing in `Win32_PnPEntity.CompatibleID`, in two forms
that answer different questions:

* `USB\DevClass_08&...` is `bDeviceClass` from the device descriptor.
  **`DevClass_00` is not "unknown"**: the specification uses it to say the class
  lives in the interface descriptors, and every one of this machine's 39 devices
  reports `00`, which is ordinary for modern hardware. A first implementation
  that read only this form scored **0 of 39** and looked like a regression.
* `USB\COMPAT_VID_046d&Class_03&...` is the *interface* class, which is the one
  that answers "what is this". 20 devices carry one. (`DevClass_` ends in the
  same five characters as `Class_`, so the second form must be matched on
  `&Class_`.)
* Hubs carry neither and get a dedicated compatible id instead —
  `USB\ROOT_HUB30`, `USB\USB20_HUB`. That is the bus driver declaring what the
  device is in a structured identifier, which is not the same thing as finding
  "hub" in a display name.

Together: **24 of 39 devices now report a class, every one from a
declaration.** The APC UPS reads `Hid`, which is correct and which no name match
produces; the AURA LED controller reads `Vendor`; the serial adapter and both
NDIS devices read `Communication`.

**A near-miss worth recording.** Deleting the dead heuristic left its
`#[cfg(target_os = "windows")]` attribute behind, where it attached to the next
function and gave `read_usb_attr` two mutually exclusive cfgs. **The Windows
build never noticed** — only Linux calls that function. The cross-target check
in the gate caught it, which is the entire reason that check exists.

### Auditing every absolute claim in the ontology against the machine

The entry below found a false invariant by accident. This ran the same check
deliberately: extract every entity whose description makes an absolute claim
(`null on`, `null when`, `never`, `always`), pull its live rows, and read the
claim against the values. 26 entities qualified.

Most held. `gpu.codec.{n}.max_fps` says it "always resolves absent" and 15 of 15
are absent. `pending_sectors` and `reallocated_sectors` say "null on NVMe" and
are. `power.battery.percentage` says "null on machines with no battery" and
reports 100% — correct, because this desktop's battery is an APC Back-UPS, which
also means the `charge_percent` fix two entries below is exercised on real
hardware.

**One did not.** `disk.{n}.read_rate` and `write_rate`, absent on all four
drives, claimed:

> Null where the platform reports only a combined figure **that cannot be
> attributed to a direction**.

No supported platform does that. Windows exposes `DiskReadBytesPerSec` and
`DiskWriteBytesPerSec`; Linux's `/sys/block/*/stat` gives read and write sectors
separately. The resolver's own absence reason, sitting on the same row, says
something different and true:

```
disk.0.read_rate = None  note="a throughput needs two samples; this query took one"
```

The counters are cumulative — `ae625ab` moved this reader to
`Win32_PerfRawData_*` precisely so they would be — so a rate requires
differencing two samples, and a single-shot snapshot has one. **The value is
obtainable by direction; it is not obtainable in one call.** The entity told an
agent the first was impossible, which would end the search, when the fix is to
sample twice.

Chased two other suspicions to ground and both were fine: the USB row count read
39 against an earlier 41 because two devices had been unplugged in the interim,
not because my new address scheme collided (39 devices, 39 addresses, 39
instances, zero unmatched templates); and `pci.{addr}.numa_node` null on all 64
devices is honest on a single-node desktop.

**Worth keeping.** Both defects this technique found are *the reason attached to
an absence*, not the absence itself. This crate is careful that a missing value
says why it is missing — which means the "why" is load-bearing prose that no
test checks and that ages badly as readers improve underneath it. **When a
reader changes how it obtains something, the entity's explanation of what it
cannot obtain is now suspect.**

### An invariant the ontology states and this machine breaks

Found by sweeping the domains that had not been read yet. Two rows, side by
side, on this desktop:

```
system.virtualization.platform          = "bare_metal"  [Measured]
system.virtualization.hypervisor        = "hyperv"      [Measured]
system.virtualization.detection_method  = "CPUID: Microsoft Hv"
```

And the entity for the second one said:

> Which hypervisor, when one was detected. **Null on bare metal**, which is an
> answer rather than a gap.

So the ontology states an invariant that the resolver breaks on the machine it
is running on. **The code is right and the documentation is wrong**, which is
the opposite of the usual direction in this file and took a moment to accept.

`detect.rs` already knows exactly why, in a comment written before this session:

> Windows 11 enables virtualization-based security by default, which puts the
> *host* under a thin hypervisor: a bare-metal desktop reports the "Microsoft
> Hv" CPUID signature exactly as a guest VM does. Vendor string alone therefore
> cannot answer "am I in a VM", and every caller that assumed it could has been
> wrong on ordinary Windows 11 hardware.

It answers the question from the Hyper-V **privilege leaf** instead, which is
why `platform` correctly says `bare_metal`. Both rows are true at once: this is
physical hardware, and a hypervisor is present. The description was written for
a simpler world than the detector already understood.

**Why a description is worth a commit here.** Nothing in the crate infers
VM-ness from `hypervisor` — I checked every consumer. The reader of that
sentence is an *agent*, and the ontology is the product. An agent that coded
`if hypervisor != null then guest` would be wrong on the default configuration
of every modern Windows desktop, and it would have been following the
documentation exactly.

**Worth keeping: the sweep that found it was looking for wrong values, and this
was two right values and a wrong sentence.** Reading rows for plausibility
catches fabrications; reading them *against their own declarations* catches a
different class, and it is the class where the code is fine and nobody is
looking.

### A flat battery that was never read, and the test that caught me

`BatteryInfo::charge_percent` was `f32`, and `battery/mod.rs` built it with
`supply.capacity_percent.unwrap_or(0)` over an `Option<u8>` that `power_supply`
had modelled correctly. The resolver then pushed it as `Reading::measured` with
no guard, so **`power.battery.percentage` resolved 0% as a measurement** on a
laptop whose gauge could not be read.

Zero is not a neutral wrong answer for this field. It means "about to shut
down". An agent asked whether it could start a long job would have said no. It
is `Option<f32>` now, `push_opt` in the resolver, and the TUI prints
"charge unread" rather than "0%".

**And then the interesting part.** Running the gate, a conformance test failed:

```
a_reading_never_resolves_weaker_than_its_entity_declares ... FAILED
```

Not the battery — the **per-core frequency from the entry above**. When I added
it I reasoned that "the entity declares Measured, Windows now publishes Derived,
nothing enforces entity-versus-reading provenance so a mismatch is honest at the
reading level". **That was wrong, and this crate has a test for it**, whose
assertion says exactly what to do instead:

> the declaration should carry the weakest provenance its rows can have

So `cpu.core.{n}.frequency` is declared `Derived` — the weakest any platform
produces — and Linux over-delivers `Measured`, which the test allows on purpose
and documents why. A `Derived` entity must name inputs that exist, which
required publishing `cpu.core.{n}.frequency.max`: the rated maximum the Windows
reader was already multiplying and then discarding.

**Two things worth keeping.** First, the failure was **order-dependent**: the
PDH query is process-global, so the test passed alone (its `snapshot()` primed
the counter and got nothing) and failed beside its siblings (something else had
primed it). It would have been easy to rerun it in isolation, see green, and
conclude the suite was flaky. Second, and worse: **I had explicitly considered
this rule, guessed it was unenforced, and shipped on the guess.** The check took
one grep. When you find yourself reasoning about whether an invariant is
enforced, go and look.

### Per-core clocks, from the counter that was named three fixes ago

`cpu.core.{n}.frequency` was absent on every Windows machine, and the reader
said exactly why and exactly what would fix it:

> Reporting the real per-core clock needs `PdhGetFormattedCounterArrayW` over
> `% Processor Performance`, which is two samples with an interval between
> them — see HANDOFF.

That is now done, and the note was right about the obstacle. `CurrentMhz` from
`CallNtPowerInformation` is the nominal clock — 4400 on all 24 logical
processors, idle and loaded — so the reader correctly reported nothing rather
than publishing a specification behind a measurement's provenance. But
`% Processor Performance` is a **rate**, and a rate needs two collections
separated in time. A single call can only get one by sleeping, and sleeping in a
monitor's refresh path is not acceptable.

So the query is opened once and kept in a `OnceLock<Option<Mutex<..>>>`, and
each call collects against the previous call's collection. **The first call of
the process returns nothing** — which is the contract the NPU utilization reader
already has, for the same reason. Verified in that order:

```
call 1:      0/24 cores report a frequency   (priming, as designed)
call 2:     24 cores, 4732-5322 MHz
under load: 24 cores, 4763-5332 MHz
```

Real per-core spread, boosting well past the 4400 nominal — the 9900X doing what
it actually does, where the field was empty before.

**The provenance needed a decision.** The value is a specification maximum
multiplied by a measured ratio, which is not a measurement. Linux reads
`scaling_cur_freq`, which is. Rather than let one entity's declared provenance
mean two different things on two platforms, `CpuFrequency` carries
`current_is_derived` and the resolver publishes `Reading::derived` or
`Reading::measured` from it. The entity description now says so outright.

**Worth keeping.** The note that made this possible was written by the commit
that *removed* the wrong value. It named the API, the counter, and the reason
the obvious implementation would not work — so the work resumed from where it
had stopped rather than from the symptom. **A deferral is only useful if it
records what the next person needs; "TODO: per-core frequency" would have cost
a day of rediscovery.**

### The last swallow, a real USB key, and a series that changed meaning

Three things that were each recorded as needing a decision, decided.

**`firmware`, the sixteenth and last swallowing reader.** The entry below
reverted a mechanical conversion of it and said the four PowerShell queries
wanted separating first. They are four methods now, and separating them made
the rule obvious: `Win32_BIOS` and `Get-PhysicalDisk` **enumerate** — both push
`FirmwareEntry` values — while `Win32_ComputerSystem` and
`Confirm-SecureBootUEFI` only **decorate** what those found. So the first two
take the any-source-succeeded rule and the second two are allowed to fail
quietly, because their resting values already mean "not established": an empty
vendor string, and `SecureBootStatus::Unknown`, which is what
`Confirm-SecureBootUEFI` produces without elevation anyway. **The conversion
was impossible to do mechanically because the function was four functions.**

**USB ids.** The entry two below left this as an id-format decision with three
candidates, each failing one requirement. Measuring the machine settled it. Of
41 USB nodes, **only 14 have a hub and port Windows will report**; 18 are
interfaces of composite devices, which share their parent's location and are
distinguished only by the `MI_xx` in their instance path; 9 are root hubs with
no location at all. So a hub-and-port id would have been stable and **collided
for two thirds of them** — worse than the unstable-but-unique index it replaced.

The id is now each platform's own device path, normalised to one segment: the
sysfs name on Linux, the PnP instance path on Windows, the `Location ID` on
macOS. 41 devices, 41 distinct addresses, and each changes only when that device
moves or is replaced. They are uglier —
`usb.usb_root_hub30_5_239a9e6d_0_0` — and that is the trade: an id a human
skims less easily, in exchange for one that does not silently repoint a device's
history when something else is unplugged.

**A claim of mine that was wrong.** That entry also said `tsdb` is keyed on
these ids and would need a migration note. It is not: `SystemSnapshot` is a
fixed-layout record with no USB data in it at all. Checking that turned up the
real migration, in a different field:

`swap_used` and `swap_total` **are** stored, and the pagefile fix three entries
below changed what they mean on Windows — from commit accounting to actual
pagefile usage, 97 GB to 3.4 GB on this host. **The layout is unchanged, so an
old file still parses**, which is exactly why `DB_VERSION` goes to 4 rather than
being left alone: a series spliced from both definitions is readable, plausible
and wrong, and nothing in the record says which definition produced a given row.
Version 3 was rejected for the same kind of reason — a stored zero that does not
say whether it was measured.

**Worth keeping.** I went looking for the consequences of an id change and found
the consequences of a *value* change I had shipped two commits earlier without
noticing it touched recorded history. **The question "what else stores this?" is
worth asking after every reader fix, not only after the ones that look
structural.**

### The last six enumerators, and what the list was really for

`audio`, `bluetooth`, `cpu_cache`, `display`, `os_info`, `power_profile`. The
list opened in `acf4e69` with sixteen modules and closes here, except for
`firmware`, which is recorded separately because its Windows reader needs
untangling first.

Nothing surprising in the six — which is the point, and is why the list took so
long to finish. **Each one still needed the same twenty-line judgement**: is
this subprocess the enumeration, or an addition to it? The answers differed
every time.

* `powercfg /list`, `pmset`, `sw_vers`, `system_profiler`, `sysctl` **ship with
  their operating systems**, so failing to run one is a failure.
* `/proc/asound/cards`, `/sys/class/bluetooth`, `/sys/class/drm`,
  `/etc/os-release` and `cpu0/cache` are **absent on kernels that lack the
  subsystem**, which is a reading, and unreadable-for-another-reason, which is
  not. `ErrorKind::NotFound` splits them.
* `power_profile`'s Linux reader has **no enumeration at all** — a governor, a
  battery, a brightness, each optional — so its `Result` is for consistency and
  nothing in it fails the read.
* `cpu_cache`'s macOS reader asks `sysctl` for five keys, and `hw.l3cachesize`
  is genuinely absent on parts with no L3, so **one key exiting non-zero must
  not fail the whole read** while `sysctl` being unrunnable must.

Five creation tests were rewritten with the others, and `cpu_cache`'s Windows
reader was replaced whole rather than spliced, after three failed attempts at
surgical brace edits on the same function. **When an edit to a function has
gone wrong twice, write the function out.**

**On the delay.** This list sat open across roughly a dozen commits while newer
findings — a webcam claiming to be streaming, a disk rate published as a total,
97 GB of imaginary swap — kept arriving and kept looking more urgent. They were
more urgent, one at a time. The list was the thing that stopped them recurring,
and deferring it a dozen times was the wrong call each time in a way that was
only obvious in aggregate.

### 97 GB of swap in use, on a machine using 3.4 GB of it

Found by dumping the `memory.*` rows and reading them:

```
memory.total       = 100547727360   (93.6 GB)
memory.used        =  52803788800   (49.2 GB)
memory.swap.total  = 154233028608  (143.6 GB)
memory.swap.used   =  97254912000   (90.6 GB)
```

**Swap in use exceeding memory in use, on an idle desktop, is not a reading a
machine can produce.** The actual figures, from `Win32_PageFileUsage`: 50 GB
allocated, **3.4 GB** in use. Off by a factor of twenty-seven, and an agent
asked "is this machine thrashing" would have said yes.

The reader:

```rust
let total_pages = perf_info.CommitLimit as u64;
let used_pages  = perf_info.CommitTotal as u64;
SwapInfo { total: .., used: .., cached: None }
```

`CommitLimit` and `CommitTotal` are **commit accounting**, not pagefile
accounting. The commit limit is RAM plus pagefile; the commit charge is every
byte of private memory the system has promised, nearly all of it resident in RAM
and never written to disk. The code renamed them `total_pages` and `used_pages`
and stored them in a struct called `SwapInfo`, and from there nothing in the
program could tell.

The `MEMORYSTATUSEX` fallback under it had the same defect wearing a more
convincing name: **`ullTotalPageFile` is documented as the commit limit, not the
pagefile size.** A field whose name contains the word you are looking for is the
easiest way to read the wrong quantity, and this is the second time this session
— `DiskReadBytesPerSec` was the first, a rate stored in a field documented as a
total.

`Win32_PageFileUsage` reports the real thing and needs no elevation. This host
now reports 50.0 GB total, 3.4 GB used.

**The cost is real and worth stating.** That is a WMI round trip:
`read_memory_stats` goes from effectively free to ~150 ms on the first call
(COM initialisation) and 20–40 ms after. It is called from the agent state and
the AI tool surface — request-driven, not a render loop — so that is affordable,
and the physical-memory figures still come from the native call. Caching the WMI
connection thread-locally would remove most of it and is not done here, because
a cached COM object that goes stale fails permanently and this crate already has
two hand-rolled COM connection strategies to get along with.

**A workflow note that cost three rebuilds.** Writing a throwaway
`examples/_probe.rs`, running it, then deleting it leaves cargo's fingerprints
inconsistent, and the next `--all-targets` build fails with
`can't find crate for simonlib` and `crate egui required to be available in rlib
format`. Combined with a disk at 99% it looks exactly like the half-written
cache from the earlier disk-full event. **Three gates in a row returned
`TESTS=101 result-lines=0` with clippy printing nothing**, which is
indistinguishable from success if only the exit code is read. Run
`cargo clean -p silicon-monitor` after probing with a scratch example, before
gating, and always count the `test result:` lines.

### Boot mode asserted because it is usually right

The first line of `firmware::refresh_windows`:

```rust
self.boot_mode = BootMode::UEFI; // Modern Windows is almost always UEFI
```

The comment is true and it is not a reading. `board.firmware.boot_mode` is
declared `Measured` and described as *"whether the machine booted UEFI or legacy
BIOS — determines whether Secure Boot is even applicable"*, so **the one machine
that cares about the answer is the one that boots legacy BIOS, and that is the
machine being told UEFI.** "Almost always" is a statement about the population;
the field is a statement about this host.

`GetFirmwareType` answers it, needs no elevation, and **this crate already wraps
it** — `platform::windows::firmware_type()`, called by `boot_config`, whose own
comment reads *"claiming Legacy on a failed query is how the old code got it
wrong"*. So the reasoning had already been worked out and applied in one module
while the module next door assumed the opposite direction. That is the same
shape as the `smart.passed` entry three below, and it is now frequent enough to
state plainly: **when a fix lands, grep for the API rather than the symptom, and
fix every caller in the same commit.** This file has said that since 5.x; it
keeps being the thing that was not done.

macOS asserted `UEFI` unconditionally too. Every Intel Mac boots EFI and none
boots legacy BIOS, so the architecture settles it there — but **Apple silicon
boots iBoot, which is neither**, and was being reported as UEFI. It is now UEFI
on `x86_64` and `Unknown` elsewhere.

Linux was already right: it reads `/sys/firmware/efi`.

This host still reports `uefi`, now from `GetFirmwareType` instead of from an
assumption that happened to hold.

**What did not get done, and why it is recorded rather than half-landed.** The
same commit was going to convert `firmware`'s enumeration to report its
failures, like `smart` above. Its Windows reader interleaves four queries — two
that add entries and two that only decorate them — and the edit got the brace
bookkeeping wrong. I reverted it rather than push a reader I had stopped being
able to read. **A module that resists a mechanical edit is telling you
something**, and in this case it is that the four queries want separating before
any of them can be made honest.

### The health reader's own enumeration could not report a failure

`smart` off the enumerator list. It is the one where the defect matters most and
also the one where the three platforms need different treatment, which is the
point of the entry.

Windows enumerates *from* `Get-PhysicalDisk`, through the usual triple
`if let Ok`. A failure produced an empty disk list, and **an empty disk list out
of a health reader reads as "no drives to worry about"**. It now propagates.

macOS enumerates from `smartctl --scan`, with `diskutil` as a fallback, and the
two need opposite handling:

* `smartctl` is **optional software**. Its absence is not a failure — there is
  no SMART source, and `diskutil` still enumerates the drives. A `smartctl`
  that *is* installed and exits non-zero is a real failure. Both were swallowed
  together.
* `diskutil` **ships with macOS**, so failing to run it is a failure, full stop.

Linux is the third case and is deliberately left alone: it enumerates from
sysfs and only *enriches* with `nvme` and `smartctl`, so a missing tool costs
attributes rather than devices. That is the `codec` exemption from the entry
this list started in, and it is the reason the list has to be worked one module
at a time rather than swept.

**Three modules, three correct answers, all of them "propagate the failure" at
first glance.** The distinction is not whether a subprocess can fail — they all
can — but whether the subprocess *is* the enumeration or an addition to it.
That question is answerable by reading twenty lines, and it is not answerable by
grep.

Behaviour on this host is unchanged: four disks, all found.

**And a process note, because it nearly went the other way.** The gate came
back:

```
TESTS=101 result-lines=0
```

Exit 101, and **not one `test result:` line** — the truncated-run signature this
file already warns about. The log said `can't find crate for simonlib` and
`crate h2 required to be available in rlib format`: a build cache half-written
when the disk filled up earlier. The clippy run just above it had also printed
nothing, which looks exactly like success. `cargo clean -p silicon-monitor` and
a real 20-minute rebuild produced the honest 15/15. **A gate that prints nothing
is not a gate that passed**, which is the same sentence as everything else in
this file, aimed at the tooling instead of the readers.

### An id documented to be stable, built from enumeration order

Two entries below left `network 20` and `usb 41` as unexplained counts. Both
turn out to be fine — the crate reports 20 of this host's 27 adapters, omitting
Teredo, 6to4 and IP-HTTPS pseudo-interfaces, and Windows genuinely has 41 USB
PnP nodes once composite devices and their interfaces are counted. **My earlier
baselines were both wrong**: `Get-NetAdapter` without `-IncludeHidden` (11) and
`Win32_NetworkAdapter` (18) are neither of them the set the reader enumerates.

Looking at the USB list for that turned up something else. The resolver:

```rust
// Bus and port rather than enumeration order: an index shifts when an
// unrelated device is unplugged, which would silently repoint every id.
let base = format!("usb.{}_{}", dev.bus_number, dev.port_number);
```

and the entity: *"The id segment is bus and port, **which survives
re-enumeration where an index does not**."* The Windows reader:

```rust
bus_number: 0,
port_number: idx as u8,
```

`port_number` **is** the enumeration order. Every id on this machine is
`usb.0_0` through `usb.0_40`, and unplugging one device repoints every id after
it — the exact failure the comment was written to prevent. This crate has a time
series database keyed by these ids.

macOS was more pointed still: it parses a real port out of the `Location ID`,

```rust
bus_number = ((loc >> 24) & 0xFF) as u8;
port_number = ((loc >> 20) & 0xF) as u8;
```

and then pushes `port_number: device_idx`, overwriting it with a counter. **Both
values were in scope on the same line.** That is fixed here — it is a
one-word change and strictly an improvement.

**Windows and Linux are not fixed, and the reason is worth recording rather
than papering over.** A correct id needs to be stable *and* unique, and the
obvious sources give up one or the other:

* Windows `LocationInformation` yields `Port_#0011.Hub_#0002` for devices on a
  hub, which is exactly right — but composite-device interfaces share their
  parent's location. Two `USB Input Device` rows on this host both read
  `0010.0000.0000.007.003.001.000.000.000`. Adopting it would make ids stable
  and **collide**, which is worse than unstable-and-unique: two devices would
  write to one id.
* Linux parses `1-4.2` and keeps only the first hop, so `1-4.2` and `1-4.3`
  both become bus 1 port 4. **Already a collision**, on the one platform whose
  ids were thought to be right.
* The full port chain fixes uniqueness but `4.2` cannot go in an id segment —
  the ids are dot-separated and `usb.1_4.2.product` would parse as an extra
  segment and stop matching its template.

So the fix is an id-format decision on an agent-facing contract, not a patch,
and it is left as open work with those three constraints written down. What is
done here is to stop the documentation asserting a property the code does not
have: the entity and the resolver comment now say which platform fills it
honestly and which does not.

**Worth keeping.** Every other entry in this file is a wrong *value*. This is a
wrong *key*, and it is the more dangerous kind: a value that is wrong is wrong
once, while an id that silently repoints attaches every future reading to the
wrong device and corrupts history that already looked correct. **When checking a
reader, check what it names things as well as what it says about them.**

### A USB stick with no SMART, passing its SMART check

Looked at because `disk.{n}.smart.passed` was a uniform family — four disks, all
`true` — and four healthy disks passing is the ordinary case. It was not that.

```
disk.0.model                       = "Linux File-Stor Gadget USB Device"
disk.0.smart.passed                = true      [Measured]
disk.0.smart.power_on_hours        = absent
disk.0.smart.power_cycles          = absent
disk.0.smart.reallocated_sectors   = absent
disk.0.smart.pending_sectors       = absent
disk.0.smart.uncorrectable_sectors = absent
```

**Every SMART counter absent, and the SMART verdict present and positive.** A
USB mass-storage gadget has no SMART at all. The row is self-refuting and it
took no hardware knowledge to see: *a verdict with no evidence under it.*

The entity is unusually precise about what the field is:

> The drive's own pass/fail verdict on itself — NVMe critical warning bits, or
> the ATA failure prediction. **Not a judgement computed from the counters
> below.**

The reader:

```rust
passed: !matches!(disk.health, DiskHealth::Critical | DiskHealth::Failed)
```

Two things wrong at once. `DiskHealth::Unknown` is documented "health could not
be determined", and it is neither `Critical` nor `Failed`, so **a drive that
reported nothing passed** — the third time this session that `!=` against the
bad value turned silence into the affirmative. And `smart::DiskHealth` is partly
a **score this crate computes from the counters**:

```rust
disk.health = if score >= 80.0 { Good } else if score >= 50.0 { Warning } ...
```

which is the one derivation the entity rules out by name. The field was the
thing it documents itself not to be, on both platforms.

`Some` now comes only from the two sources the entity names — the NVMe log
page's critical warning bits, and the ATA failure prediction — and the Windows
WMI fallback and the whole Linux path answer `None`, because neither
`smartctl -H`'s self-assessment nor the NVMe warning byte is captured there at
all.

```
before: PhysicalDrive3 (Usb)  Health Passed: true
after:  PhysicalDrive3 (Usb)  Health Passed: the drive did not give a verdict
        PhysicalDrive1 (Nvme) Health Passed: yes      ← from its own warning bits
```

**Worth keeping: the crate already had this argument with itself and won it
once.** `disk.{n}.health` — the platform's verdict, a different field — maps
`Unknown` to an absence, under a comment reading *"`Unknown` is the absence of a
verdict, not a verdict of 'unknown'"*, and there is a test named
`a_drive_with_no_readable_counters_is_not_graded_healthy`. The reasoning was
written down, the guard was built, and the field one line over was left with the
opposite behaviour. **A principle applied in one place is not applied; the
sibling that shares its data is where to look next.**

### Every NVMe device counted twice, once as its own controller

Two entries below fixed a display reader that enumerated adapters, and one below
that an audio reader that enumerated codecs. Rather than wait for a third to
turn up, the same question was asked of every family at once: **how many
instances does each report on this machine, and is that number right?**

```
board.audio     4     board.display   1     cpu.core      24
disk            4     disk.controller 9     memory.dimm    2
gpu             3     pci            64     usb           41
```

`disk.controller 9` on a machine with three NVMe drives is the one that does not
add up, and the listing said why:

```
Standard NVM Express Controller   iface=SCSI  driver=stornvme  pci=PCI\VEN_144D&DEV_A810…
Standard NVM Express Controller   iface=SCSI  driver=stornvme  pci=PCI\VEN_144D&DEV_A808…
Standard NVM Express Controller   iface=SCSI  driver=stornvme  pci=PCI\VEN_144D&DEV_A80C…
Samsung SSD 990 PRO 4TB           iface=NVMe  driver=nvme      pci=""
Samsung SSD 970 EVO Plus 2TB      iface=NVMe  driver=nvme      pci=""
Samsung SSD 9100 PRO 4TB          iface=NVMe  driver=nvme      pci=""
```

**Each NVMe device appears twice** — once as its actual controller, once as the
drive itself. The second set came from a `MSFT_PhysicalDisk WHERE BusType = 17`
query, and `MSFT_PhysicalDisk` is what `Get-PhysicalDisk` wraps: it enumerates
**disks**. The empty `pci_address` on those three rows is the tell, and it was
sitting in the output the whole time — *a controller without a bus address is
not a controller*. Everything that branch published is disk identity, which
`disk.{n}` already reports from a reader that knows it is describing a disk.

And the three real controllers were labelled **SCSI**:

```rust
self.parse_windows_controllers(&text, StorageInterface::SCSI);   // Win32_SCSIController
...
let interface = if name.to_lowercase().contains("nvme") { NVMe } else { default_iface }
```

The default is *the class of the WMI query that returned the row*, which is not
a property of the device — Windows exposes NVMe controllers through
`Win32_SCSIController`. The name check that was supposed to rescue them looked
for `nvme`, and Windows calls them `Standard NVM Express Controller`: **the one
spelling of the word that Windows does not use.** Meanwhile `stornvme` sat in
the `DriverName` field of the same row. The bound driver names the transport, so
that is what the classification reads now.

```
before: 9 controllers, 3 of them disks, 3 real NVMe controllers labelled SCSI
after:  6 controllers, the 3 NVMe ones labelled NVMe
```

**Worth keeping: counting is a cheap detector and nobody had done it.** Three
enumeration defects in one session, and all three announce themselves in a
single number. The check is one query against a live snapshot, it needs no
knowledge of the reader, and the question it asks — *does this machine really
have that many?* — is one a person can answer by looking at the machine.

**Its actual hit rate, recorded honestly, because the paragraph above reads
better than the evidence supports.** Five counts looked wrong. One was:
`disk.controller`. Two were **right and nearly "fixed"**:

* `gpu 3` reported `NVIDIA GeForce RTX 3090 Ti` twice, which is exactly the
  duplicate-row shape that had just been found in three other readers. This
  machine has two of them — `nvidia-smi` lists bus `01:00.0` and `03:00.0`. The
  audio enumeration had even hinted at it, in the `2- ` prefix Windows adds to a
  second instance of an adapter, which two entries below is cited as a *nuisance*
  for joining. It was evidence.
* `cpu.cache 3` looked short for a CPU with split L1. The sizes check out for a
  12-core Zen 5 — 960 KB of L1 is 12 × (48 KB data + 32 KB instruction), 12 MB of
  L2 is 12 × 1 MB, 64 MB of L3 — and `Win32_CacheMemory` aggregates per level by
  design.

Two more, `network 20` against 18 adapters and `usb 41` against 38 PnP devices,
are small discrepancies that plausibly come from counting loopback and root hubs,
and were left alone rather than guessed at.

**So the detector fires about as often on correct code as on broken code, and
the difference is only visible by checking the machine.** That is not an
argument against running it — one of the five was a real double-count nobody had
noticed. It is an argument against acting on it directly: *a suspicious count is
a question, and the answer comes from the hardware, not from the pattern.* Had
the `gpu` family been "fixed" by deduplicating on name, this file would now
contain a confident entry about a defect that never existed, and the crate would
report one GPU on a two-GPU machine.

### Twelve audio endpoints where four exist, two facing backwards

The display entry below ends by saying a wrong enumeration is a wrong value in
every field of every row. `audio` is the same defect in the same shape, found by
going looking for it, and it is worse in one specific way.

The reader concatenated `Win32_SoundDevice` — audio **adapters** — with the PnP
`AudioEndpoint` class, giving twelve rows on this host. Eight are codecs and
controllers: `Realtek High Definition Audio`, `AMD Streaming Audio Device`,
`NVIDIA High Definition Audio` twice. The entity reads "endpoint name **as the
platform presents it to a user**", and a user is shown none of them.

Direction came from a regular expression over the name:

```powershell
$isInput = $dev.Name -match 'Microphone|Input|Capture|Line In'
```

**It was inverted on half of the real endpoints.** A virtual audio interface
names its endpoints from the application's point of view, so `MOTIV Mix Virtual
Input` is what you play *into* — a **render** endpoint — and `MOTIV Mix Virtual
Output` is what you record *from*. The registry says Render and Capture
respectively; the regex said the opposite of both, with complete confidence, and
`board.audio.{n}.direction` published it as `Measured`.

That is the thing worth keeping. Every other fabrication in this file is a
missing reading dressed as a present one — a zero, a default, an absence with
nowhere to go. **This one is a present reading pointing the wrong way**, and no
amount of `Option` would have helped: the field was populated, non-null, and
exactly backwards. What catches it is not a type but asking where the value came
from, and "a regex over a human-readable label" is an answer that should end the
conversation.

And the default endpoint was whichever row came first:

```rust
let is_default = if is_output && !has_default_output { .. true } else { false };
```

which on this machine named an audio *controller* as the default output.

The endpoint list Windows itself presents is kept — four rows, no duplicates —
and joined to `MMDevices\Audio\{Render,Capture}` on the GUID that ends the PnP
device id, which is exactly the registry subkey name. **An equality on a unique
key, not a name match**: the friendly names do not join, because Windows
disambiguates a second instance of an adapter by prefixing `2- `, and
`LG ULTRAWIDE (2- NVIDIA High Definition Audio)` matches nothing in the
registry. I tried the name join first and it failed on one of four, which is
exactly the hit rate that makes a heuristic look like it works.

Before and after:

```
before: 12 devices; 8 of them adapters; MOTIV Virtual Output = Output, Virtual Input = Input
after:   4 endpoints; MOTIV Virtual Output = Input, Virtual Input = Output; state from
         DeviceState; default absent
```

`DeviceState` is masked with `0xF` — the documented states are a four-bit field
and Windows sets further undocumented bits above them. That also makes
`board.audio.{n}.state` a real reading for the first time; the entry two below
notes that three of its four values were unreachable.

### Three displays where one exists, two of them graphics cards

Recorded as open work two commits ago on the grounds that the fix needed a
dependency feature and a multi-monitor path that could not be verified here.
Both halves of that turned out to be softer than they looked, so it is fixed.

The defect, visible in a snapshot without reading any code:

```
board.display.0.name = "LG ULTRAWIDE"
board.display.1.name = "AMD Radeon(TM) Graphics"
board.display.2.name = "NVIDIA GeForce RTX 3090 Ti"
```

The reader looped over `Win32_VideoController` — graphics **adapters** — and
pasted monitor details onto them by array index:

```powershell
foreach ($ctrl in $controllers) {
    $mon = [PSCustomObject]@{ Name = $ctrl.Name; ... }
    if ($monitorDetails -and $idx -lt @($monitorDetails).Count) {
        $mdet = @($monitorDetails)[$idx]      # positional, not a join
```

So the display *count* was the adapter count, and name, brightness and
connection type were each attributed to whichever adapter happened to sit at the
same index. **Display 0 having the right name was a coincidence of ordering.**

`EnumDisplayDevices` answers the question that was actually being asked. On this
machine it enumerates **fourteen** display devices and flags exactly **one** as
`ATTACHED_TO_DESKTOP` — the other thirteen are outputs the drivers expose with
nothing plugged in. `EnumDisplaySettings` then gives that device's current mode.

The EDID metadata still comes from `root\wmi`, but joined on the **hardware
id** rather than an index: `GSM76F6` appears in both
`MONITOR\GSM76F6\{guid}\0001` and `DISPLAY\GSM76F6\5&2a745970&0&UID4352_0`.
Two monitors of the same model share that id — and they also share their EDID
name, connection family and panel size, so **the ambiguity cannot reach any of
the three values taken from it.** Brightness is per-instance and would need the
UID, so it is not taken from that join at all.

Before and after, on this host:

```
before:  3 displays; names "LG ULTRAWIDE", "AMD Radeon(TM) Graphics", "NVIDIA GeForce RTX 3090 Ti"
after:   1 display; LG ULTRAWIDE (GSM), HDMI, primary, 3440x1440@60, 800x340 mm, 32 bpp
```

Physical size is new — nothing published it before.

**Two things about the process.**

The reason to enumerate the right objects is not tidiness. Every one of the
three fabricated properties came from the same root cause: **the collection was
of the wrong kind of thing, so everything attached to it had to be guessed.** A
sentinel is one wrong value; a wrong enumeration is a wrong value in every field
of every row.

And I damaged the file while fixing it. Extracting the untouched Linux reader
with `subprocess.run(..., text=True)` decoded git's UTF-8 output as cp1252, and
two em-dashes went back in double-encoded — the three UTF-8 bytes of an
em-dash re-read as three cp1252 characters and re-encoded. (Writing the
mangled sequence here as a literal mangled it a second time, which is its
own small demonstration.) It survived `cargo fmt`, `clippy`, 864
tests and both cross-target checks, because a comment is not code. It was caught
by diffing the two functions I had *not* meant to change against `HEAD` — which
is worth doing after any splice, and is the second self-inflicted encoding
mangle this session. **A tool that rewrites a file it only meant to move is
indistinguishable from one that reads it correctly, until something compares
bytes.**

### A device's name read as the speed it negotiated

`usb.{addr}.speed` was one of the uniform families three entries down — six
devices, all `"super"`. It was left alone at the time because a machine whose
USB devices are all on USB 3 hubs would legitimately look like that. It is not
that.

The entity:

> **Negotiated** bus speed — low, full, high, super. A super-speed device on a
> high-speed port reports high, **which is how a wrong cable shows.**

The Windows reader, under its own accurate comment:

```rust
// Determine speed from class heuristic
let speed = if pnp_id.contains("USB3") || name.contains("USB 3") || name.contains("xHCI") {
    UsbSpeed::Super
} else if name.contains("USB 2") || name.contains("EHCI") { UsbSpeed::High }
  else { UsbSpeed::Unknown };
```

Those strings describe what the device **is**. The entity asks what it
**negotiated**. A USB 3 device plugged in through a USB 2 cable keeps `USB3` in
its PnP path and was reported as `Super` — **the reader is wrong in exactly the
case the field exists to expose**, and right only when nothing is wrong. That is
the same shape as `printer.accepting_jobs` two entries below, and it is now the
sixth time this session: *the description names the distinction, and the reader
computes the thing the distinction rules out.*

The tell was visible in the output all along, without reading any code:

```
USB Root Hub (USB 3.0)        speed=Super
Generic SuperSpeed USB Hub    speed=Super
```

**A device whose name contains "SuperSpeed" reporting super speed is not
evidence of anything.** When a reading can be predicted from the label next to
it, it is the label.

Windows genuinely can answer this —
`IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX` on the parent hub returns a
`Speed` field — and until something calls it, `Unknown` is the honest value. The
resolver already had the right sentence waiting for it: *"the platform did not
report a negotiated bus speed"*.

macOS parses a real `Speed:` line from `system_profiler`, and got two things
wrong around it: the running value initialised and reset to `Full`, so a device
with no `Speed` line reported full speed, and the parse's `else` arm was `Full`,
so any string it did not recognise did too. Full speed is a real value some
devices negotiate, which is what made it a bad default. Both are `Unknown` now
and only a literal `12 Mb` reads as `Full`.

### Nine tests asserting a contract the readers no longer make

`3606758` failed CI on **both** Linux and macOS:

```
printer::tests::test_printer_monitor_creation ... FAILED
    assert!(monitor.is_ok())
```

Working as designed. CI runners have no print scheduler, `lpstat` exits
non-zero, and the reader now says so instead of reporting zero printers. But the
test asserted the old contract, and **eight more tests assert it in the eight
other modules converted this session** — one `assert!(monitor.is_ok())` each,
every one a CI failure waiting for a runner that takes the failing path. Rather
than meet them one push at a time, they are all rewritten here.

The old assertion was true *by construction*: `refresh` could not fail, so
`is_ok()` was a tautology dressed as a test. The new contract is weaker and
worth something:

```rust
match PrinterMonitor::new() {
    Ok(_monitor) => {}
    Err(e) => assert!(e.to_string().len() > 10, "failed without saying why: {e:?}"),
}
```

**Whatever happens, the caller can tell which happened** — and a failure has to
carry a reason, because a reason is the whole difference between "this machine
has none" and "nobody looked".

The `lpstat` call also gained the distinction that `input` and
`storage_controller` already had, and it is the right one here:

* `lpstat` is **not installed** — CUPS is absent, so there are no CUPS printers.
  A reading. `Ok` with an empty list.
* `lpstat` **runs and exits non-zero** — the scheduler is not answering. Not a
  machine without printers.
* It runs and prints a list.

**Worth keeping.** Making a reader honest changes its contract, and every test
that encoded the dishonest contract will fail — on the platform that exercises
it, which is not necessarily the one in front of you. After converting a reader,
grep its module for `is_ok()`, `unwrap()` and `expect()` before pushing:

```bash
grep -n "is_ok()\|\.unwrap()\|expect(" src/<module>/mod.rs | grep -i "new()\|monitor"
```

That is the same lesson as the nullable-flag entry two below — a green local gate
is a statement about this machine — arriving through a different door. **Twice in
one session the thing that broke was not the code but a test that had been
asserting the absence of a capability the code just gained.**

### "On battery" concluded from nobody having asked

The entry below deferred this one to open work on the grounds that it needed a
type change reaching the GUI and the observability API. That was over-cautious:
`PowerSupplyInfo::online` never leaves `power_supply.rs`, and only its three
accessors do. The whole change is four call sites.

Three defects, all the same shape as the one above:

```rust
ac_info.online = status.ACLineStatus == 1;   // 255 means unknown
...
pub fn on_ac_power(&self) -> bool {
    self.supplies.iter().any(|s| s.supply_type == Mains && s.online)
}
```

`ACLineStatus == 255` is documented as **unknown** and became `online: false`.
`on_ac_power()` returns `false` when *no mains supply was enumerated at all* —
so a failed enumeration and a machine running on battery were the same answer.
And `on_battery()` was `!self.on_ac_power() && has_battery`, which turns that
`false` into a positive claim: **"not on mains" was concluded from "nobody said
whether it is on mains".**

Downstream, `collect_power_info` rendered it as `status: "Battery"` and the
module's example printed `🔋 Running on Battery`. On a desktop with a UPS whose
line status reads unknown, that is a monitoring tool announcing a power failure.

All three now return `Option<bool>`. `on_ac_power` answers `Some(true)` if any
mains supply says it is online, `Some(false)` only if one actually said so, and
`None` when none of them said anything — including when the list is empty.

**Worth keeping: the negation is where the damage happens.** `online` being
wrong is one field. `on_battery = !on_ac_power` propagates a missing reading
into an affirmative claim about a *different* thing, and it is the affirmative
claim that reaches a user. Anywhere a three-valued fact is collapsed to two,
check what the `!` downstream turns the collapsed value into — this session has
now found it in `!is_secure_boot()` recommending Secure Boot be enabled,
`!on_ac_power` suppressing battery advice, and `!on_ac_power()` announcing a
machine is on battery.

The rest of `enumerate_supplies` is careful — 255 handled for
`BatteryLifePercent`, `0xFFFFFFFF` for both time fields, 128 for "no battery" —
which is what made these two lines worth fixing rather than the function worth
rewriting. **A reader that gets four sentinels right and one wrong is more
dangerous than one that gets none right**, because the four earn it a
credibility the fifth does not deserve.

`BatteryMonitor`'s hand-written `Default` existed to seed `ac_connected: true`;
with the field an `Option` it is a derive.

### The same inequality again, this time against a string

The grep the entry below adds — a health check written as `!=` against a bad
value — returns nothing else in enum form. It returns something in string form
one file over, which the grep does not match and which is the same defect:

```rust
// (Get-CimInstance Win32_Battery).BatteryStatus
// 2 = AC, 1 = battery
self.on_ac_power = text != "1";
```

A desktop has no battery and the query returns nothing, which is not `"1"`. A
query that **fails** also returns nothing, which is also not `"1"`. The field
was a `bool` initialised to `true`, so all three of "on mains", "no battery" and
"nobody asked" were the same value.

`on_ac_power` gates every battery recommendation the module makes —

```rust
if !self.on_ac_power { recommendations.push("Consider disabling CPU boost on battery…") }
```

— so the failure mode was **silence about a laptop's power settings while it ran
on battery**. Not a wrong reading printed to a user; a right reading withheld
because a wrong one was believed.

`GetSystemPowerStatus` asks the kernel and has a value for the third case:
`ACLineStatus` is 0 offline, 1 online, **255 unknown**. The reader now maps all
three and the field is `Option<bool>`. This host reports `Some(true)`.

**The lesson is narrower than "use an enum".** `text != "1"` and
`status != PrinterStatus::Offline` are the same mistake at different type
levels, and the enum did not prevent it. What both have in common is that the
comparison is against the *bad* value rather than for the good one, so every
unanticipated input — including the empty one that means "no answer" — lands on
the affirmative side. **Compare for what you mean to find.** Written as
`text == "2"` the string version would have been correct by construction, and
the desktop case would have surfaced as the genuine question it is.

### A field derived from the one thing it exists not to be derived from

`printer`, next off the swallow list, and the field it carries is the cleanest
example yet of a pattern this session has now hit five times.

The entity:

> Whether the queue is taking new work. **Distinct from `status`**: a stopped
> queue may still accept jobs and hold them, which is the difference between a
> delayed print and a rejected one.

The Windows reader:

```rust
accepting_jobs: status != PrinterStatus::Offline,
```

**Derived from `status`.** The description does not merely fail to describe the
code — it states the exact derivation that is wrong and explains why, one file
away from the line that performs it. CUPS reports the real thing (`lpstat -a`
prints "accepting requests"), `Win32_Printer` has no equivalent property, and
the reader filled the gap by computing the forbidden thing rather than leaving
it empty.

On this machine the consequence is visible without any special case:

```
Brother MFC-L2900DWXL Printer    status=Unknown  accepting=None   (was: true)
```

The WMI status mapping is careful — `PrinterStatus` values 1 and 2 map to
`Unknown` rather than being guessed at. So for this printer the crate knew it did
not know the status, and **still published a confident `accepting_jobs: true`,
because `Unknown != Offline`.** A negative comparison against an enum turns
every unrecognised value into the affirmative case; that is the same defect as
`_ => AudioState::Active` two entries down, written as an inequality instead of
a match arm. **Any `x != Enum::Bad` used as a health check has this shape.**

Worth adding to the greps at the bottom of this file:

```bash
grep -rnE "!= [A-Za-z]+::(Offline|Unknown|Error|Failed|Disabled)" src/ --include=*.rs
```

`refresh_cups` had the swallow too — `Err(_) => return` on `lpstat`, which exits
non-zero when the scheduler is not running. A machine whose print scheduler is
down and a machine with no printers reported the same empty list.

### Sockets counted as NUMA nodes, memory divided evenly between them

`numa` was next on the swallow list and turned out to have three separate
inventions stacked in it.

**Sockets are not nodes.** The Windows reader built one node per entry in
`Win32_Processor` — one per socket. A single AMD socket presents one to four
NUMA nodes depending on the firmware's NPS setting, and a two-socket Xeon with
sub-NUMA clustering presents four. The kernel answers the actual question in one
call, `GetNumaHighestNodeNumber`, which needs no elevation and no subprocess.

**Memory divided evenly.** Under a comment reading `// Get memory per node
(approximation: divide total evenly)`, each node got `TotalVisibleMemorySize /
node_count` stored in `memory_total_bytes` and published as
`memory.numa.{n}.memory`, declared **`Measured`** and described as "memory
attached to this NUMA node". Which memory is attached to which node is the whole
point of the field — an even split is not an approximation of it, it is the
assumption the field exists to refute.

But the division is sound in exactly one case, and it is worth stating why:
**if there is one node, all the memory is attached to it.** Not an
approximation — an identity. So the reader now fills `memory_total_bytes` when
`node_count == 1` and leaves it unknown otherwise. This is the third time this
session that an aggregate has been sound evidence for one case and none for the
rest; the webcam entry below is the same shape.

`GetNumaAvailableMemoryNodeEx` *does* give a real per-node figure, and it is
free memory rather than installed. The reader now reports it — **a reading that
did not exist before**, on every machine, however many nodes.

**And the fallback.** `refresh()` ended with:

```rust
if self.nodes.is_empty() { self.create_uma_fallback(); }
```

pushing a node 0 with zeroed memory and a distance matrix of `[10]` — the SLIT
convention for "local". A machine whose topology could not be read was published
as single-node UMA with a fabricated distance matrix, and `memory.numa.is_numa`
answered `false` as a **measurement**. It now runs only from the Linux branch
that has established there is no node directory, where one node really is the
answer; every other failure propagates.

macOS was asserting `memory_used_bytes: total_mem` — every byte on the machine
in use.

After, on this host:

```
nodes=1 is_numa=false
  node 0 cpus=24 total=100547727360 free=60627513344 used=39920214016
```

**One thing worth keeping about the process.** The first run of that printed
`total=98191140` — 98 MB on a 93 GB machine. `MemoryStats::ram.total` is in KB
and I had read it as bytes. Nothing in the type system objected, both are `u64`,
and the value would have been published as `memory.numa.0.memory` in bytes with
`Measured` provenance. **It was caught by looking at the number**, which is the
same check this whole session has been applying to other people's code and is
worth applying to one's own within the same hour. A plausibility test would not
have caught it either: 98 MB is a perfectly plausible quantity of memory.

### The same field, right on two platforms and invented on the third

Following the webcam entry below, a sweep of every field derived from a WMI or
PnP `Status` string, since that is where the last three "right answer to the
wrong question" findings came from. `board.input.{n}.active` came back — and it
is a useful negative result, because on two of three platforms it is correct.

The entity asks a precise question:

> Whether the platform reports the device as connected and usable. A present but
> inactive device is a different fact from an absent one — **a Bluetooth
> keyboard out of range is still enumerated.**

Windows answers it with `Win32_PnPEntity.Status == "OK"`, which really is
"connected and usable" — the *same expression* that was wrong for
`board.camera.{n}.active`, because there the question was "is it streaming".
**The defect was never the expression; it was the pairing.** Linux reads the
`Handlers` line, also right.

macOS sets the literal `true` at both construction sites, including the
Bluetooth branch — the one case the description singles out. An out-of-range
Apple keyboard was reported as connected and usable.

Now `Option<bool>`: `Some` from a real read on Linux and Windows, `None` on
macOS. This host is unchanged, all six devices still `Some(true)`, which is the
correct outcome for a change that only removes an invention.

**Two things worth keeping.** First, a sweep that returns mostly correct code is
not a wasted sweep — knowing that `Status == "OK"` is right *here* and wrong
*there* is what turns a pattern into a rule. Second, this one had to be caught
by reading: the entity id is built with `format!`, so
`entities_with_an_absence_path_are_declared_nullable` could not see it and the
`nullable` flag had to be flipped by hand. **The static check's blind spot is
exactly the per-instance entities, which are most of them.** Anyone converting
another field to `Option` should assume the flag needs flipping and check.

The macOS branch is correct by inspection and unverified — there is no Mac here.

### A green gate that could not see the defect it shipped

`f1470d5` passed the full local gate — fmt, clippy, both cross-target checks,
864 lib tests, 73 doc-tests, all fifteen suites — and failed on the macOS runner:

```
board.audio.0.state is declared non-nullable but resolved unavailable
```

Making the audio state absent-capable needed the entity's `nullable` flag
flipped, and I flipped it for `board.camera.{n}.active` in the same session and
not for this one. **Windows is the only platform that produces a value for
`board.audio.{n}.state`, so the machine running the gate could not take the path
that breaks.** The cross-target `cargo check` compiles the macOS branch; it
never runs it. A local gate on one operating system is structurally blind to
this whole class, and no amount of running it again would have helped.

Chasing that, a scan of every literal id passed to `Reading::unavailable` or
`push_opt` against the entity's `nullable` flag found **five more**, none of
them mine:

```
cpu.model               cpu.cores.physical      cpu.total.utilization
memory.utilization      system.uptime
```

Each has a deliberate absence path with a written reason — *"an empty model
string is a failed read, not a CPU without a name"*, *"total memory reported as
zero, so a percentage has no denominator"* — sitting under an entity declaring
it is never null. They have been latent for as long as those readers have
existed, waiting for a machine that takes the branch. **A conformance test that
inspects the readings can only ever check the paths this machine happened to
take;** every branch not taken is a claim nobody has tested.

So the new test reads the resolver's *source* rather than its output:
`entities_with_an_absence_path_are_declared_nullable`. Every id that appears as
a literal beside `Reading::unavailable` or `push_opt` must belong to an entity
that admits being absent. It is platform-independent by construction, it fails
on a machine that would never take the path, and it was checked against a
deliberately reverted `cpu.model` before being trusted. It cannot see ids built
with `format!`, which is most of the per-instance ones, so it is a floor and not
a ceiling.

**Worth keeping, and it generalises past this crate.** Two tests, same
invariant: one samples behaviour and one reads structure. The behavioural one is
more faithful — it checks what actually happened — and it can only report on the
subset of the program that ran. **When a property must hold on every platform
and the gate runs on one, the check has to be static, or it is not a check of
the property but of the platform.**

### An idle webcam reporting that it was streaming

The second unrun technique from this file: **two snapshots six seconds apart,
listing every `Measurement`-kind reading that did not move.** Most of what it
returned is legitimately still on an idle host — SMART counters, power-on hours,
two idle GPUs. One row was not:

```
board.camera.0.active = true
board.camera.1.active = true
```

The entity, written before this session:

> Whether the camera is streaming right now. **The one genuinely live field in
> this cluster**, and the reason it is a measurement rather than an identity.

It is live on exactly one of three platforms. Linux probes the device node.
macOS sets the literal `true`. Windows used:

```rust
is_active: item["Status"].as_str() == Some("OK"),
```

`Win32_PnPEntity.Status` is Device-Manager health — **`"OK"` is what a working
camera that is switched off reports**. So both webcams on this machine were
published as streaming, continuously, and would have been on any Windows host
with a functioning camera. For a privacy signal that is the wrong way round: the
field exists to answer "is something watching me", and its answer was yes.

**Windows does publish a real signal**, and it is readable without elevation:
`CapabilityAccessManager\ConsentStore\webcam` records `LastUsedTimeStart` and
`LastUsedTimeStop` per application, and a `LastUsedTimeStop` of zero means that
application still has the device open.

It is per-application, not per-device — so it is used **in one direction only**:

```rust
match (saw_a_key, any_open) {
    (true, false) => Some(false),  // nothing has a camera open, so this one is not streaming
    (true, true)  => None,         // something is, but not which device
    (false, _)    => None,
}
```

**The negative generalises to every device and the positive generalises to
none.** That asymmetry is worth naming on its own: an aggregate signal can be
sound evidence for one of its two answers and no evidence at all for the other,
and a reader that publishes both is right half the time by construction. The
same shape appeared in the measured-boot entry below — a log that evidences
`Some(true)` and cannot evidence `Some(false)` — pointing the opposite way.

This machine now reports what is true of it:

```
Lenovo 510 IR Camera  active=Some(false)
Lenovo 510 RGB Camera active=Some(false)
```

**Not yet verified: the positive path.** Confirming `LastUsedTimeStop == 0`
while an application streams means opening the user's webcam, which is not
something to do unasked. The negative path is verified here; whoever next has a
camera app open should check that `active` becomes `None` rather than `false`.

Also worth noting what the technique cost: the two-snapshot diff produced about
sixty rows and fifty-nine of them were fine. **A detector with a low hit rate is
still worth running when the misses are cheap to dismiss and the hit is a webcam
that says it is watching you.**

### Fifteen codecs, one frame rate: a constant with a derivation

This file has recommended a detection technique since 5.x and nobody had run it:
**group the readings by entity family and flag a family whose instances all
report the same value.** Thirty lines against a live snapshot:

```
board.audio.{n}.state          x12 = "Active"
board.input.{n}.interface      x6  = "USB"
gpu.codec.{n}.max_fps          x15 = 60
usb.{addr}.speed               x6  = "super"
system.printer.{n}.status      x3  = "Idle"
disk.{n}.smart.passed          x4  = true
...
```

Not every uniform family is a defect — four healthy disks really do all pass
SMART. But **a family with fifteen members and one value is a constant that has
learned to wear a per-instance name**, and three of these were.

**`gpu.codec.{n}.max_fps` is the sharpest thing found this session.** The entity:

```rust
P::Derived,
"Frames per second at the maximum resolution. An estimate in every case ...
 no driver reports a frame rate, so this is arithmetic over the engine
 generation.",
).derived(&["gpu.codec.{n}.codec", "gpu.codec.{n}.max_resolution"])
```

The reader: `max_fps: 60,` at all twelve construction sites. **There is no
arithmetic.** The entity does not merely carry a wrong value — it declares a
computation, and names the two inputs that computation consumes, and neither
input is read, and the output cannot vary. `max_resolution` *does* vary across
those fifteen rows, which is the proof: a value derived from something that
varies cannot be constant. 60 is not even a safe floor — no engine does 8K60 on
every codec.

And in front of it, one more unreachable guard:

```rust
match c.max_fps {
    0 => out.push(Reading::unavailable(..)),   // never taken
    fps => out.push(Reading::derived(..)),
}
```

That is the fourth `== 0` guard this session standing in front of a reader that
cannot produce zero. **`Derived` is a weaker claim than `Measured` and it is
still a claim**: it says a calculation happened. It now resolves absent, and the
entity says why.

**`board.audio.{n}.state`** was `AudioState::Active` literal on Linux and macOS.
Windows genuinely read something — and read the wrong thing:
`Win32_SoundDevice.Status` is Device-Manager health, while the entity is
described as endpoint state, "active, disabled, unplugged, not present". Its
match handled three of WMI's eleven status values and sent the rest, `Unknown`
and `Pred Fail` included, to `_ => Active`. `Degraded` was mapped to `Idle`,
which is an activity state and not a health one. Now `OK` and `Error` map, and
everything else is absent. (The real endpoint state is a `DeviceState` DWORD
under `MMDevices\Audio`, readable unelevated; noted for whoever reworks that
enumeration.)

**`board.input.{n}.interface`** reported `USB` for any device id containing
`hid`, under a comment that already said so: *"a bare `HID` instance names the
device class, not the transport — this reports USB for both, which is a guess."*
The comment was right and the code did it anyway; that is the fourth time this
session. Now only a `USB` in the instance path counts, and `Unknown` resolves
absent through the guard `push_str_as` already had.

After, on this machine — the input family is no longer uniform, which is the
point:

```
board.input.0.interface = "USB"     board.input.2.interface = absent
board.input.1.interface = "USB"     board.input.3.interface = absent
gpu.codec.*.max_fps     = absent x15
board.audio.*.state     = "active" x12   (all twelve really do report Status=OK)
```

**Worth keeping.** The audio row is the useful one: its published value did not
change at all. What changed is that it is now a reading rather than a coincidence
— and on a host where one endpoint reported `Pred Fail`, the old code would have
printed the same word. *A uniform family is not proof of a defect and a correct
value is not proof of a reading; the question is always whether any input could
have made the output different.*

### SIP read as Secure Boot, and advice from an unread flag

Taking the rule from the entry below — for a security property, `Some(false)` is
a far stronger claim than `None` — and applying it as a sweep. Three sites, on
three platforms, in one field.

**Windows.** `boot_config` reads Secure Boot from
`SYSTEM\CurrentControlSet\Control\SecureBoot\State`, and the line above it is a
paragraph this file already recorded: an earlier fix that stopped reporting
every UEFI machine as Secure Boot. Directly underneath it:

```rust
self.boot_info.secure_boot =
    crate::platform::windows::secure_boot_enabled().unwrap_or(false);
```

`secure_boot_enabled` returns `Option<bool>` *precisely because* the registry
value can be missing. **The defect was put back one line under the paragraph
describing it.**

**Linux.** `data.last().copied().unwrap_or(0) == 1` in `boot_config`, and
`if data.last() == Some(&1) { Enabled } else { Disabled }` in `firmware`. The
efivar's last byte is 1 or 0; an empty read is neither, and both readers called
it off. `firmware` even has a `SecureBootStatus::Unknown` variant sitting unused
on that branch — *where an enum has an `Unknown` variant, look for the sibling
branches that don't use it.*

**macOS, which was measuring a different thing entirely.**

```rust
let output = Command::new("csrutil").arg("status").output();
self.boot_info.secure_boot = stdout.contains("enabled");
```

`csrutil status` reports **System Integrity Protection**. SIP is a runtime
kernel protection; Secure Boot is a boot policy. An Apple silicon Mac can run
SIP with its boot policy set to Reduced or Permissive — which is exactly the
configuration someone auditing Secure Boot is looking for — and this reported it
as Secure Boot enabled. **Not a sentinel, not a default: the right answer to the
wrong question.** No amount of `Option` fixes that; the field is now `None` with
`bputil -d` and the `boot-policy` NVRAM variables named as what would have to be
read.

And the consequence, from the module's own example:

```rust
if !monitor.is_secure_boot() {
    println!("   ⚠️  Secure Boot is disabled");
    println!("      Consider enabling for better security");
}
```

**A machine whose Secure Boot flag was never read was told to turn Secure Boot
on.** That is the whole session in four lines: the absence had nowhere to go, it
became `false`, `false` reads as a finding, and the finding became advice. The
example now says nothing when it knows nothing.

This host is UEFI with `UEFISecureBootEnabled = 0`, so it still reports Secure
Boot disabled — the reading was right here, and right for the reason the value
happens to be readable, which is not the same as the code being right.

### Measured boot reported off, on a host where it is on

The follow-up the entry below set aside. `TpmInfo` carried three detail fields
and every platform fabricated all three.

```rust
// windows, privileged path
let mut algorithms = vec!["SHA-1".into(), "SHA-256".into()];
if version == TpmVersion::V2_0 { algorithms.extend(["RSA".into(), "ECC".into()]); }
...
pcr_banks: 24,
measured_boot: true, // Windows with TPM implies measured boot
// windows, registry fallback
algorithms: Vec::new(), pcr_banks: 0, measured_boot: false,
// macos
algorithms: vec!["AES-256".into(), "SHA-256".into(), "ECC-P256".into()],
pcr_banks: 0, measured_boot: true,
```

**The algorithm lists are specifications talking, not devices.** Every one of
them is what a TPM of that version, or a Secure Enclave, is *documented* to
support — correct as a statement about the standard and unverified as a
statement about this chip. Linux was the one platform that really read them,
from the driver's `pcr-*` directories, and then replaced an empty result with
the standard TPM 2.0 list under a comment saying "Default for TPM 2.0". **A
reader that falls back to the specification when the device says nothing has
stopped being a reader.**

`pcr_banks: 24` is the constant that appears in every TPM 2.0 tutorial. The
other two paths answer `0`, which is not a bank count a TPM can have — so the
field's three possible values were an invented number, a number no device
reports, and on Linux a real count that collapsed to `0` when the directory
would not open.

**And `measured_boot` was wrong on this machine, in the direction that matters.**
The unelevated path is the registry fallback — the privileged WMI query is
denied without elevation — so this host published:

```
board.tpm.measured_boot = false   [Measured]
```

Measured boot is **on** here. A security check reading that field would have
concluded platform integrity measurement was disabled on a machine where it is
running. The `true` on the other branch is no better: it is an inference from
"a TPM exists", and a machine can have a TPM with measured boot switched off.

There is a real signal, and it took one look to find: the boot loader writes the
TCG log to `C:\Windows\Logs\MeasuredBoot`, and this host has four current
files there. So the reader now evidences the positive and **refuses to evidence
the negative** — `Some(true)` or `None`, never `Some(false)`, because a cleared
directory and a policy that discards logs both look like an absence. The same
asymmetry applies on Linux, where `/sys/kernel/security/ima` existing is
evidence and its absence is not.

After, on this machine:

```
tpm: Unknown/Unknown algorithms=None pcr_banks=None measured_boot=Some(true)
  board.tpm.measured_boot = Some(Bool(true)) [Measured]
  board.tpm.present       = Some(Bool(true)) [Measured]
  board.tpm.status        = None [Unavailable] "a TPM is present but whether it is enabled ..."
```

**Worth keeping.** For a security property, `Some(false)` is a much stronger
claim than `None` and is almost never the one a reader is entitled to make.
"Measurements are not active", "Secure Boot is off", "no unmitigated
vulnerabilities", "no TPM" — each of those needs a source that can distinguish
absence from silence, and where no such source exists the honest field is
three-valued.

### "This machine has no TPM", published as a measurement

`tpm` and `storage_controller`, the next two off the list.

The TPM resolver was already written for the distinction, and said so:

```rust
// Not knowing whether a TPM exists is different from knowing there is
// none, so `present` goes unavailable here rather than false.
Err(e) => { out.push(Reading::unavailable("board.tpm.present", ..)) }
...
// A successful enumeration that found nothing is a reading: this machine
// has no TPM. That is exactly the case `present` exists to state.
out.push(Reading::measured("board.tpm.present", json!(monitor.has_tpm()), None));
```

Both comments are right. **The premise underneath them was false**:
`TpmMonitor::refresh` returned `Ok(())` unconditionally, so the `Err` arm could
not be reached by anything except a constructor failure, and every detection
failure arrived at the second branch and was published as
`board.tpm.present = false`, **`Measured`**. A security posture check reading
that field learns "this host has no TPM" from a query that never ran.

That is the fabrication this whole session keeps meeting, at its worst so far:
the value is a `bool`, so there is no room for an absence; the sentinel is
`false`, which is the comforting-looking answer; and the field is one a
compliance report would act on.

**The fix had to preserve a failure that is correct.** Unelevated on this
machine:

```
Get-CimInstance -Namespace 'root/cimv2/Security/MicrosoftTpm' -ClassName Win32_Tpm
  Access denied     (exit 1)
Test-Path 'HKLM:\SYSTEM\CurrentControlSet\Services\TPM'
  True
```

The privileged query is *expected* to fail for an ordinary account — the code's
own comment says "requires admin, but attempt anyway" — and the registry check
is what answers. So a blanket `?` on the first query would have turned a working
detection into an error on every non-elevated run. The rule is the one from
`usb`: **carry the first failure, let the second source decide, and raise only
when every source failed.** After the change this machine still reports
`present=true`.

`storage_controller`'s `query_wmi_json` reached `Option<String>` through three
`.ok()?` — COM initialization, the namespace connection, and the query — so a
machine whose WMI was unreachable and a machine with no SCSI controller were the
same value. Its three Windows queries now follow the same any-source-succeeded
rule, and this machine reports what it has:

```
controllers: 9 -> [.., "Samsung SSD 990 PRO 4TB [NVMe]", "Samsung SSD 970 EVO Plus 2TB [NVMe]", ..]
```

**Worth keeping.** Three times now the code contained a correct, explicit
statement of the distinction — `sensors`' probe, the TPM resolver's two
comments, `DiskInfo`'s `Option<u32>` sector sizes — and was wrong anyway,
because the statement lived one layer away from the code that decided. *Prose
and types both describe intent; only the branch that runs decides.* When a
comment names a distinction, the thing to check is not whether the comment is
right but whether any code path can actually produce both of its outcomes.

### Two enumerations, both failing, reported as one empty

Converting `usb` and `input`, the next two off the list in the entry below.

`usb::refresh_windows` had a shape none of the others did — a fallback:

```rust
if let Ok(devices) = Self::wmi_enumerate_usb() { self.devices = devices; }
if self.devices.is_empty() {
    if let Ok(devices) = Self::registry_enumerate_usb() { self.devices = devices; }
}
```

Two independent enumerations, and **a fallback is exactly the structure that
makes a swallowed failure hardest to see**: the second source exists because the
first is known to be unreliable, so the code already expects to arrive at the
bottom with an empty list, and cannot tell "both said nothing" from "neither
answered". The rule now is that **either source succeeding is enough to trust an
empty result, and both failing is not an empty machine** — the error names both
reasons.

`input` had four of the plain shape across three platforms. Its Linux reader
also shows why "propagate everything" is not the rule either:

```rust
Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
Err(e) => return Err(..),
```

**A file that is absent and a file that will not open are different answers.** A
kernel without `/proc/bus/input/devices` has no input layer, which is a reading;
a permission error on the same path is not. The same split appears in every
sysfs reader converted so far, always as `Path::exists()` guarding the
enumeration and `read_dir` propagating.

`codec` was on the list and comes off it: its Windows and macOS readers are
empty stubs and every capability it publishes is already tagged
`CapabilitySource::Inferred`, so a failed `vainfo` or `lspci` costs an optional
source rather than producing a false claim about the hardware. **The list was
built by grepping for a shape; one of the sixteen did not have the defect that
shape usually carries.**

Verified on this machine after the change:

```
usb: 39 devices
input: 6 -> ["Enhanced (101- or 102-key)", ..., "HID-compliant mouse", ...]
```

### The same swallow in sixteen more enumerators

The PCI defect below was not one bug. Counting the shape across the crate:

```bash
grep -rn "fn refresh_\(windows\|linux\|macos\)(&mut self) {" src/ --include=*.rs
```

**Forty-eight readers in sixteen modules**, every one returning `()` into a
`refresh()` that returns `Result` and can only ever answer `Ok`. audio,
bluetooth, camera, codec, cpu_cache, display, firmware, input, numa, os_info,
power_profile, printer, sensors, smart, storage_controller, tpm, usb.

This commit adds `core::command` — `capture`, `capture_json`, `json_items` —
where a spawn failure, a non-zero exit and non-UTF-8 output are each an error
and *only* an empty stdout is "nothing there" (PowerShell's `ConvertTo-Json`
prints nothing at all for an empty result set, so that shape really does mean
it). Then it converts the first two modules. The remaining fourteen are listed
under open work.

**`sensors` already knew.** Its `refresh_windows` opened with this:

```rust
// If the helper cannot be run at all, that is recorded: "no sensors" and
// "could not ask" are different, and this path used to return the same
// empty list for both.
let probe = Command::new("powershell").args(["-NoProfile","-Command","exit 0"]).output();
if let Err(e) = probe { self.last_note = Some(..); return; }

if let Ok(output) = Command::new("powershell").args([.. the real query ..]).output() {
```

A whole spawn of `powershell -Command "exit 0"`, existing only to detect
whether spawning works — and then the real query on the next line swallowed its
failure exactly as before. **The distinction was correctly named, correctly
argued in a comment, and then drawn in the one place it could not help.** The
accessor it fed, `note()`, carried a doc comment describing the guarantee the
code did not provide.

And with the failure no longer swallowed, the reader spoke up:

```
sensor enumeration failed: powershell exited exit code: 1
```

`MSFT_Sensor` **does not exist** in `root/standardcimv2` on Windows 11 —
`Get-CimClass` there answers `Invalid class`, and PowerShell exits 1 even under
`-ErrorAction SilentlyContinue`. That query has never returned a sensor on such
a host. **It went unnoticed for as long as it did precisely because its failure
was swallowed**; the fix and the bug it exposes are the same fix. Asking for the
class first keeps the two answers apart, and this machine now reports what is
true of it:

```
cameras: 2 -> ["Lenovo 510 IR Camera", "Lenovo 510 RGB Camera"]
sensors: 0 -> []
pci devices: 64
```

**Worth keeping.** A swallowed failure does not just lose one reading; it hides
the code path that produces it, so the reader can be dead for years while
looking healthy. Every entry in this file so far has been about a value that was
wrong. This one is about a value that was never computed, and the difference was
invisible from the outside.

### A failed enumeration reported as an empty machine

Found by a test going red, once, and only under the full fifteen-suite run:

```
---- resolution_is_stable_across_calls ----
entity coverage differed between two snapshots
  first only:  ["pci.<none>"]
  second only: ["pci.{addr}.class", "pci.{addr}.device", ... ]
```

Two snapshots of the same machine, seconds apart: the first found no PCI
devices, the second found nine entities' worth. It did not reproduce in three
isolated runs or in a fourth at four threads — **only under the load of every
suite at once.**

The temptation is to call that a flaky test. The cause is in the code:

```rust
fn refresh_windows(&mut self) {
    if let Ok(output) = Command::new("powershell").args([...]).output() {
        if let Ok(text) = String::from_utf8(output.stdout) {
            if let Ok(val) = serde_json::from_str::<Value>(&text) {
```

Three `if let Ok` with no `else`, no check of `output.status`, and a `refresh()`
that returned `Ok(())` whatever happened. A spawn failure, a non-zero exit,
empty stdout and unparseable JSON all left `self.devices` empty and reported
success. The resolver then did the honest thing with a dishonest input:

```rust
if monitor.devices().is_empty() {
    out.push(Reading::unavailable("pci.<none>", None,
        "no PCI devices enumerated on this machine"));
```

**The absence was reported with a reason, and the reason was false.** This
whole session has argued that an honest absence beats a confident wrong answer;
this is the case where the absence *is* the confident wrong answer. Publishing
`Unavailable` is not enough on its own — the reason attached to it is a claim
like any other, and here it claimed a fact about the hardware ("this machine has
no PCI devices") when the truth was a fact about the process ("PowerShell did
not run").

Both `refresh_linux` and `refresh_macos` had the same shape. All three now
return `Result`, and each distinguishable failure gets its own error, which
`resolve_pci` already had an arm for — `"PCI enumeration failed: {e}"`. Only one
path still returns an empty list as success on each platform, and on each it
means what it says: Linux, no `/sys/bus/pci/devices` at all; Windows, a
`ConvertTo-Json` that printed nothing for an empty result set.

**Two things worth keeping.** A reader that swallows failure produces a defect
whose frequency depends on machine load, so it will be rare on a developer's
box and rare in CI and not rare in a datacenter. And `resolution_is_stable_-
across_calls` — a test that only ever asserts a snapshot equals another snapshot
of the same machine, with no expected values in it at all — is what caught it.
**A test that knows nothing about the hardware can still catch a reader lying
about the hardware,** by asking it the same question twice.

### A rate published as a total, for the wrong drive

The `Some(<literal>)` grep recommended two entries below turned up
`disk/windows.rs`:

```rust
read_time_ms: Some(0),
write_time_ms: Some(0),
queue_depth: Some(0),
```

Chasing where those zeros came from found three defects stacked on each other,
each of which hid the next. All three are confirmed against this machine.

**A rate stored in a field documented as a total.** The reader queried
`Win32_PerfFormattedData_PerfDisk_PhysicalDisk`, whose `DiskReadBytesPerSec` is
what its name says: an instantaneous rate. It was assigned to
`DiskIoStats::read_bytes`, whose doc comment reads "Total bytes read since
boot", and `gui/app.rs` renders it with `format_bytes` and `ai_api/tools.rs`
publishes it as `"bytes_read"`. Linux fills the same field from
`/sys/block/*/stat`, which really is cumulative. **The same struct field held a
different physical quantity depending on the platform** — and no type could
have caught it, because both are `u64` bytes.

Two samples three seconds apart on this idle host:

```
FORMATTED  bytes=0              ops=0
RAW        bytes=3874044762112  ops=68170018   (+98304 bytes, +5 ops in 3s)
```

A machine that had read 3.87 TB since boot reported `0.00 GB`. **The wrong
answer was zero, so it looked like the honest absence this whole session has
been arguing for.** `Win32_PerfRawData_PerfDisk_PhysicalDisk` carries the same
property names as raw cumulative counters.

**An instance filter that matched nothing.** The query was
`WHERE Name LIKE '%{index}'` — the index as a *suffix*. The instances are named
`0 C:`, `1 E:`, `2`, `3 D:`. Verified here:

```
LIKE '%0' matches:            (nothing)
LIKE '0%' matches:   0 C:
```

So on any machine whose drives have letters the filter matched nothing, every
call fell through to the `_Total` fallback, and **each disk reported the whole
machine's I/O as its own**. The fallback is now gone: a drive with no instance
returns an error, because the sum of every other drive is not a worse reading
of this drive, it is a reading of something else.

**And the zeros.** `read_time_ms`, `write_time_ms` and `queue_depth` were
hardcoded `Some(0)` while the very class being queried published
`PercentDiskReadTime`, `PercentDiskWriteTime` and `CurrentDiskQueueLength`. The
two timers are `PERF_PRECISION_100NS_TIMER`, so the raw value is busy time in
100-ns units.

After, run against this machine, matching the PowerShell reading per drive:

```
Disk: PhysicalDrive1 (NvmeSsd)   Samsung SSD 990 PRO 4TB
  Read:       176.40 GB (354996 ops)
  Read Time:  476221 ms
```

**A fabricated value is easiest to spot when it is implausible, and these were
plausible.** Zero bytes on an idle disk, zero service time, zero queue depth —
every one of them is what a quiet disk really looks like. The tell was not the
value, it was that the *code could not have known it*: nothing in
`read_io_counters` ever asked for a service time.

The same commit removed `rotation_rate: Some(7200) // Common HDD speed`, an
invented RPM for every Windows drive not already known to be an SSD.

### The doc comment said not to assume 64 bytes; all three readers did

Continuing the non-zero-default sweep into `cpu_cache` and `io_scheduler`.

The ontology entity for `cpu.cache.{n}.line_size` carries this description,
written before this session:

> Cache line size. The unit of false sharing, and the reason this is exposed
> per instance rather than assumed to be 64 bytes.

Every reader assumed exactly 64 bytes. Linux `.parse().unwrap_or(64)`, Windows
`item["LineSize"].as_u64().unwrap_or(64)`, macOS a `read_sysctl` closure ending
in `.unwrap_or(0)` whose result was cast and stored unchecked. 64 is x86's line
size; Apple silicon's is 128 — so the one platform where the assumption is
wrong was also the one that could publish a zero.

And the resolver in front of it:

```rust
(cache.line_size > 0).then(|| serde_json::json!(cache.line_size)),
```

A `> 0` guard, and the value it guards against is 64. This is the third time
this session the same pair has appeared: a guard written against a zero
sentinel standing in front of a reader that defaults to something else.

`associativity` was worse than defaulted. Its own doc comment reads
`0 = fully associative` — zero is a **real, distinct value** — and the readers
filled unread associativity with `unwrap_or(0)`, macOS with a literal
`associativity: 0` on every cache of every Mac. The sentinel and the
measurement were the same number, so no guard anywhere could have separated
them.

**Where a field documents its own sentinel as meaningful, the type has already
run out of room.** Nothing short of `Option` can fix it, and the doc comment
naming the collision is the tell.

`io_scheduler::BlockDeviceIo` had the block-size defaults of `f5a54ee`
(`unwrap_or(512)` twice) plus two consequential ones:

```rust
let rotational = Self::read_sysfs_u32(&queue_path.join("rotational")).unwrap_or(0) == 1;
let discard_support = ... .map(|v| v > 0).unwrap_or(false);
```

An unreadable `rotational` read as **SSD**, and `scheduler_optimal()` is
defined entirely by that flag — HDDs want BFQ, SSDs want none. So an unread
flag did not merely lose information, it produced a scheduler recommendation
for the wrong class of device, printed as advice. `scheduler_optimal()` now
returns `Option<bool>` and a device with no flag is counted neither optimal nor
not.

Note the shape of `discard_support`: `.map(|v| v > 0)` builds the `Option`
correctly and `.unwrap_or(false)` throws it away on the same line. **The
absence was constructed and then discarded** — the mirror image of `f5a54ee`,
where it was wrapped back up in `Some`.

### An `Option` filled with a sentinel wrapped in `Some`

Sweeping for **non-zero** defaults — the ones the entry below shows a `> 0`
guard cannot catch — found a variant worth its own name.

`DiskInfo::physical_sector_size` and `logical_sector_size` were already
`Option<u32>`. The type was right. Every reader on every platform filled it with
an invented value and wrapped that in `Some`:

```rust
// linux
.read_sysfs_u64("queue/physical_block_size").unwrap_or(512) as u32
...
physical_sector_size: Some(physical_block_size),
// windows
physical_sector_size: Some(512),
logical_sector_size:  Some(512),
// macos
physical_sector_size: Some(4096),
logical_sector_size:  Some(512),
```

512 is the common value and not this drive's: a 4Kn drive reports 4096 for
both, and an Advanced Format drive 512 logical over 4096 physical — **which is
exactly the distinction the two fields exist to carry**. macOS asserted the
Advanced Format layout for every disk on every Mac. A third field,
`block_size: u32`, carried the same number under `// Most common`.

**The absence had somewhere to go and the reader filled it anyway.** Every other
finding this session was a type that could not express "unknown"; this is the
opposite failure, and no amount of making types honest prevents it. `Some(512)`
reads as a measurement to every consumer and to every guard, including one
checking `> 0`.

**So the `Option` migrations that make up most of this session's work are
necessary and not sufficient.** Worth grepping for after any of them:

```bash
grep -rnE "Some\([0-9]" src/ --include=*.rs        # literal wrapped in Some
grep -rnE "unwrap_or\([0-9]" src/ --include=*.rs   # then re-wrapped
```

### A guard that named the danger, defeated by a sentinel of 64

The sharpest version of the compensating-consumer pattern, and the one that
should change how the earlier ones are read.

`DimmInfo::is_ecc()` is `total_width_bits > data_width_bits` — ECC is 72 bits
carrying 64. Both widths defaulted: Linux and Windows set `data_width` to **64**
and `total_width` to whatever `data_width` came out as, and macOS hardcoded both
to 64. Equal widths mean no ECC, so **a machine with ECC memory reported that it
had none** whenever the widths were not read, and always on macOS.

The ontology resolver had already spotted this, and its comment is exactly
right:

> ECC is the widths differing, so it can only be stated when both are known.
> **Two zeros are equal, and would otherwise report "no ECC".**

And the guard it wrote was `if dimm.total_width_bits > 0 && dimm.data_width_bits > 0`.

**The sentinel was not zero. It was 64.** Both defaults sail through a `> 0`
test, so the resolver published `ecc: false` as a `derived` reading for every
DIMM whose widths it had not read — while carrying a comment explaining why it
must not.

**A `> 0` guard defends against a zero sentinel and nothing else.** Every
compensating consumer found this session — `observability/api.rs`'s
`total_gpu_memory_bytes > 0`, the TUI's two, `resolve.rs`'s `family > 0` — is
sound only because those particular sentinels happened to be zero. A default of
64, or 3500, or 4400, or 1200 walks straight through. That is the argument for
fixing sentinels at the source rather than guarding them downstream: the guard
has to know which value was invented, and it usually assumes the most obvious
one.

`profile/memory.rs` had no guard at all and published `ECC = false` as a
setting row.

### Three helpers, one module, and only one of them right

Acting on the rule from the entry below — *a sentinel feeding a threshold
comparison silently answers "under the threshold"* — pointed at the modules that
compute conclusions. `health.rs` has three convenience functions over the same
fallible check, and they disagree with each other:

```rust
pub fn health_status() -> HealthStatus {
    SystemHealth::check().map(|h| h.status).unwrap_or(HealthStatus::Unknown)  // right
}
pub fn health_score() -> u8 {
    SystemHealth::check().map(|h| h.score).unwrap_or(0)                        // wrong
}
pub fn has_critical_issues() -> bool {
    SystemHealth::check().map(|h| h.has_critical()).unwrap_or(false)           // wrong, and
}                                                                              // reassuringly so
```

**The module already had the right answer.** `HealthStatus::Unknown` exists and
is used by the first function; the other two, written beside it, defaulted
instead. `has_critical_issues` reported **no critical issues** when nothing had
been checked; `health_score` reported **0**, the worst possible health, which
errs alarming rather than reassuring but is still a real score a caller cannot
distinguish from a measured one.

Both are `Option` now. `examples/health_check.rs` rendered the zero as an empty
bar reading `0/100`; it prints "not run".

`watchdog::timeout_secs` went with them: `unwrap_or(0)` published a watchdog
with a **zero-second timeout**, which would fire immediately and is not a
configuration any device holds, and `is_default_timeout` compared it against 30
and 60 to answer `false`.

**Where an enum already has an `Unknown` variant, look for the sibling functions
that do not use it.** The vocabulary for the absence existed here and was one
line away from each site that ignored it.

### An unread thermal zone is not a cool one

Fifth module off the callerless list, and the same shape as the security one:
the reassuring answer is the one a zero produces.

`ThermalZoneInfo::temp_mc` was `i64`, filled with
`read_sysfs_i64(zone/temp).unwrap_or(0)` — so a zone whose `temp` file could not
be read held **0 millidegrees**. Then:

```rust
pub fn is_throttling(&self) -> bool {
    self.trip_points.iter()
        .filter(|tp| tp.trip_type == TripPointType::Passive)
        .any(|tp| self.temp_mc >= tp.temp_mc)
}
```

`0 >= passive_trip` is false, so **an unmeasured zone reported that it was not
throttling**. The trip points had the same defect from the other direction:
their temperatures also defaulted to 0, and a trip point at 0 makes every zone
look tripped.

`temp_mc` is `Option<i64>` and `is_throttling` is `Option<bool>` — with no
temperature there is no answer, and `false` is the answer that gets acted on.
`hottest_temp_c` is `Option<f64>` too: `unwrap_or_default()` reported a machine
with no readable zones as sitting at 0 °C rather than as unmeasured.

**Three modules in a row now where the fabricated value was the comforting
one** — no unmitigated vulnerabilities, Secure Boot enabled, nothing throttling.
That is not a coincidence about this crate: a sentinel is usually zero or
`false`, and for a *risk* reading, zero and false both mean "fine". Anywhere
`unwrap_or(0)` feeds a comparison against a threshold, the default silently
answers "under the threshold".

### Secure Boot from a file's existence, and hardening nobody checked

`security_mitigations` is the fourth module off the callerless list, and the
findings need separating by severity, because they are not equal.

**Live, on Linux.** `detect_hardening` read Secure Boot as:

```rust
let secure_boot = Path::new(".../SecureBoot-8be4df61-...").exists();
```

The efivar is **present on every UEFI system whether Secure Boot is on or off** —
its last byte carries the state. So any UEFI Linux machine reported Secure Boot
enabled and took +3 on the posture score for it. `boot_config.rs` already reads
the same variable correctly, one module over. Beside it, three `read_sysctl(..)
.parse().unwrap_or(0) > 0` calls turned an absent tunable — older kernels lack
some of these — into "hardening switched off", and `stack_protector: true`
carried the comment *"Most modern kernels have this"*.

**Latent, off Linux, and I nearly reported it as live.** The Windows and macOS
branches assert `kptr_restrict`, `dmesg_restrict`, `unprivileged_bpf_disabled`
and `stack_protector` as `true` — Linux kernel tunables with no equivalent
there — and two of them add to the posture score. But
`detect_vulnerabilities` already returns `UnsupportedPlatform` on every
non-Linux target, so `new()` fails before `detect_hardening` is ever called and
none of it reaches a user. The comment on that refusal is the best writing in
the crate:

> Returning an empty list meant `unmitigated()` reported zero, which reads as
> "this machine has no unmitigated CPU vulnerabilities" when it means "nothing
> was checked". Of every absence in this crate that one is the least acceptable
> to guess at: **a reassuring security reading is acted on.**

All of it is `Option` now, and the test that matters is not about today's
behaviour: `a_platform_without_vulnerability_data_publishes_no_posture` fails
the moment someone implements vulnerability detection for another platform,
because the hardening branch beside it goes live in that same commit.

**The general point.** A fabricated value behind an unreachable branch is not
harmless — it is waiting for the branch to become reachable, and whoever makes
that change will be thinking about vulnerabilities, not about four booleans
twenty lines away. Latency is a property of today's call graph, not of the code.

### The first 7.0.0 item, closed

`cpufreq::is_turbo` was recorded below as the first entry queued for the next
major version — "returns `bool` where it means 'cannot tell'". It is `Option<bool>`
now, and the change was smaller than the write-up: two call sites, both in an
example.

Reading it again turned up more than the recorded diagnosis. The `false` branch
was known; the **middle** branch was not:

```rust
} else if let Some(max) = self.cpuinfo_max_freq_khz {
    // Assume turbo if current > 95% of max
    self.current_freq_khz > (max * 95 / 100)
```

**95% of the maximum is not a test for turbo.** A part whose non-turbo ceiling
*is* its `cpuinfo_max_freq` reads as boosting whenever it is busy; one boosting
to 96% of max reads as boosting for a reason unrelated to its base clock. Turbo
means *above base*, and without a base frequency there is no answer — so the
whole function is now the one comparison that is true: `current > base`, and
`None` when `base_frequency` is absent, which is every governor but
intel_pstate.

**A recorded item is worth re-reading before closing it.** The entry named the
`false`; the guess sitting above it had been described only as "guesses from
'>95% of max'" and not as wrong, and it is the branch that would fire on most
Intel machines.

### A CPUID family of zero, published by the struct the ontology protects

Third module off the callerless list. `CpuMicroarchReport` published
`family: 0, model: 0, stepping: 0` on Windows, from:

```rust
// Windows doesn't easily expose CPUID family/model via WMI in the same format
Ok((model_name, 0, 0, 0, flags, cores, threads))
```

The comment states the problem and the return contradicts it. **Family 0 is not
a value any modern x86 CPU reports** — a Ryzen 9 9900X is family 0x1A — so the
report carried an impossible triple rather than an absent one.

**The ontology was already right, and that is the point.** `resolve.rs` tested
`report.family > 0` before publishing, under a comment reading *"All three go
together or none of them do"*, so `simon snapshot` correctly showed
`cpu.microarch.family unavailable`. A library consumer reading
`CpuMicroarchReport` directly got the zeros. **Fifth instance this session of a
consumer guarding a sentinel its source should not have made**, and the guard is
now `family.is_some()` — asking the question instead of inferring it.

The same reader also returned `Win32_Processor.Name` untrimmed, so `model_name`
carried the WMI field's fixed-width padding:
`"AMD Ryzen 9 9900X 12-Core Processor            "`. That padding was trimmed in
`hardware_ai` earlier this session and left here; an earlier sweep checked
`silicon/windows.rs` for the same defect and cleared it, but never looked at
this module.

Linux and macOS read the triple properly and now return `Option` rather than
`unwrap_or(0)` — on Linux `0` was serving as both "not seen yet" in the parse
loop *and* a publishable value, which is why the loop guarded `if family == 0`.

### Every AMD CPU classified as Intel, by the word in "12-Core"

Second module off the callerless list, and the clearest single wrong answer of
the session. `interconnect::infer_topology`:

```rust
let is_intel = upper.contains("INTEL") || upper.contains("CORE") || upper.contains("XEON");
let is_amd   = upper.contains("AMD") || upper.contains("RYZEN") || ...;
if is_intel { infer_intel(...) } else if is_amd { infer_amd(...) }
```

**AMD writes the core count into the model string.** "AMD Ryzen 9 9900X
**12-Core** Processor" uppercases to contain `CORE`, `is_intel` is tested first,
and every Ryzen, EPYC and Threadripper took the Intel branch. This desktop
reported its Zen 5 chiplet CPU as a **ring bus** with **MESIF** coherence —
Intel's topology and Intel's protocol — with every bandwidth zero and
`generation: "Unknown"`. `infer_amd` was never reached on any AMD machine.

`CORE` was there to catch "Core i7" and "Core Ultra"; both still are, by tokens
a core count cannot produce. AMD is tested first regardless, since its own name
is unambiguous. After the fix the same machine reports Infinity Fabric, two
CCD→IOD links, MOESI and "IF 4.0" — all correct.

**Then the thing the fix exposed.** With the vendor right, `cores_per_die` read
**8** on a CPU with 12 cores over 2 CCDs. The table held the generation's
*maximum* CCD size, so every partially-enabled part in the range was wrong: a
9900X is 2x6, a 9600X is one CCD of 6 and was reported as 2x8. It is derived
now — `physical_cores / compute_dies`, from `Win32_Processor.NumberOfCores`,
`/proc/cpuinfo` pairs, or `hw.physicalcpu` — and `None` where the count cannot
be read. The same machine now reports `Some(6)`.

**A note on verifying the test, because it nearly fooled me.** Reintroducing the
`contains("CORE")` token alone did *not* fail the new test — the reordering
fix is independently sufficient, so the test passed against half the old code.
Only reverting **both** halves failed it, with
*"AMD Ryzen 9 9900X 12-Core Processor was classified as Intel"*. A regression
test confirmed against a partially-reverted fix proves less than it appears to:
revert the whole change, not the part you were thinking about.

### The callerless-module sweep, and what it found

The `io_info` and `drm_monitor` findings shared a shape worth turning into a
check: **public API with no internal consumers is exactly where running the
binary cannot reach.** Enumerating it is one command:

```bash
for m in $(grep -oP '^pub mod \K\w+' src/lib.rs | sort -u); do
  refs=$(grep -rl "crate::$m::" src/ --include=*.rs | grep -v "^src/$m" | wc -l)
  [ "$refs" -eq 0 ] && echo "$m"
done
```

**Twenty-six modules** come back: `anomaly`, `bandwidth`, `cgroup_monitor`,
`cpufreq`, `datacenter`, `dma_engine`, `drm_monitor`, `fleet`, `gpu_topology`,
`hardware_ai`, `health`, `interconnect`, `interrupt_map`, `io_scheduler`,
`iommu`, `memory_management`, `pcie`, `predictive`, `process_tree`,
`prometheus`, `scheduler`, `security_mitigations`, `thermal_zone`,
`voltage_regulator`, `watchdog`, `wsl`. Two of them had already yielded defects
this session without the list existing.

Crossing that list with the admission-comment grep ranks them, and the top hit
was real. `gpu_topology::estimate_link_bandwidth` returned a flat `(6, 25.0)`
for **any** NVLink pair and `(2, 23.0)` for any xGMI pair, under comments
reading *"Typical config: 6 or 12 links"* and *"typically 2-4 links"*, so
`total_bandwidth_gbs` read 150 GB/s between any two NVLink GPUs.

The split that matters: **25 GB/s per NVLink is a genuine specification of the
link type** and is the same on every board using it. The link *count* is a
property of the specific board — an A100 has 12, an RTX 3090 has 4 — so a
typical value is not this pair's value. The per-link figure stays; the count is
`Option<u32>` and `None` until something calls
`nvmlDeviceGetNvLinkState`. `total_bandwidth_gbs` follows it.

**Checked and sound in the same pass:** `predictive` names its estimates
(`Estimated time to issue in hours (None if cannot predict)`) and already uses
`Option`, which is what the rest of this list should look like.

### A link's capacity, published as the traffic on it

`silicon`'s `IoController` has `bandwidth_mbps` — documented "current bandwidth"
— and `max_bandwidth_mbps`. Two platforms filled the first with the second.

On Linux the NVMe path computed `cur_speed * cur_width * 1000 * 0.98462`, which
is the capacity of the **negotiated PCIe link**, and assigned it to
`bandwidth_mbps`. An idle NVMe drive reported about 3900 MB/s of I/O. On macOS
the Thunderbolt row was blunter still — `bandwidth_mbps: 5000.0, // TB4 = 40
Gbps` — so an idle port reported a saturated bus. **Neither is a sentinel zero;
both are fabricated *high* values**, which is worse, because a reader concludes
the device is busy rather than idle.

The ceilings were mostly invented too. Only the Linux NVMe path derived one from
the device's own link. Everything else assumed the fastest variant of its class
and published it as *this* device's maximum: 3500 for every disk on Windows
regardless of bus, 2500 for any USB controller (USB 3.2 Gen 2x2, when a USB 3.0
controller is 500), 600 per SATA *port* on a row that is a controller, 7000 and
5000 on Apple. A SATA SSD given a 3500 MB/s ceiling is wrong by about six times.

Both fields are `Option<f64>`. The negotiated-link figure survives as the
*maximum*, which is what it is and a genuinely useful one — a device that
supports PCIe 4.0 x4 but trained at 3.0 x2 cannot exceed what it trained at.
Three Linux rows carried `bandwidth_mbps: 0.0` beside comments saying the figure
"would need USB traffic monitoring"; those are `None`.

**Two things worth carrying:**

- **`io_info` has exactly one consumer in the tree, and it is an example.** Like
  `drm_monitor`, which is where the fifth `AdapterRAM` reader hid, this is
  public API that running the binary never reaches. Both defect clusters found
  this session in code with no internal callers were found by *reading the
  module*, not by running anything.
- The Windows rows are `Win32_PerfFormattedData_PerfDisk_PhysicalDisk`
  instances, whose `Name` is `"0 C:"` — a disk index and drive letters. The
  `name.contains("NVMe")` and `contains("SSD")` branches beside them **cannot
  ever match**, so every row has always been "Storage". Left as-is, because
  "Storage" is true of all of them, but the dead branches are now marked as
  dead rather than looking like working classification.

### One number, copied into twenty-four cores

`silicon::windows::read_cpu_utilization` was:

```rust
// Use overall system utilization for all cores (simplified)
let overall_util = self.read_cpu_utilization_percent();
(0..self.cpu_count as u32).map(|id| (id, overall_util)).collect()
```

`read_cpu_utilization_percent` is `GetSystemTimes` — a **whole-machine** figure
— copied into every core's entry, so a 24-core machine reported the same number
twenty-four times as though each had been measured. That is `24a7314`'s macOS
defect ("the system-wide figure repeated across every core") on a second
platform, in a second module. Linux is unaffected: it reads real per-cpu lines
from `/proc/stat`.

`CpuCore::utilization` and `CpuCluster::utilization` are `Option<u8>` now, and
Windows contributes an empty map, so each core reads `None`.

**And an honest failure worth recording, because the next person will be
tempted by the same idea.** Wiring the system-wide figure to the *cluster*
average looked right — a cluster spanning every core is exactly what a
whole-machine number describes. It reported **100% on an idle desktop** where
`simon cli cpu`, reading `core::cpu`, said 7.3% at the same moment. Probing
`GetSystemTimes` directly over the same window gives deltas that check out —
kernel+user = 120,156,250 ticks, which is exactly 12.0s across 24 cores in
500ms — and an idle delta implying ~35%.

**Explained, later in the same session, and the sentence above contains the
proof it missed.** `lpKernelTime` *includes* idle time: kernel and user together
partition *all* CPU time, so `kernel + user` is the total, not the busy part.
Measuring it directly:

```
cores=24  idle=36250000  kernel=54218750  user=65781250
kernel + user    = 120,000,000
wall x cores     = 120,000,000     <- identical, every time
```

A reader that uses `kernel + user` as its busy numerator is therefore dividing
the whole pie by itself and gets **exactly 100% on every machine at every
load** — idle desktop, saturated build server, no difference. Sampling both
readers over one shared window settles it:

```
round 1: correct 97.91%   core::cpu 97.48%   kernel+user-as-busy 100.00%
round 2: correct 97.03%   core::cpu 97.18%   kernel+user-as-busy 100.00%
round 3: correct 96.71%   core::cpu 96.60%   kernel+user-as-busy 100.00%
```

`(kernel + user - idle) / (kernel + user)` agrees with `core::cpu` to within
half a percent. The removal was right, and the reason is now known rather than
merely suspected.

**Worth keeping: "the arithmetic checks out" was the bug report.** The original
note recorded that kernel+user came to *exactly* wall-clock times core count and
read that as a sanity check passing. It is the tell. **If a quantity you believe
is a subset comes out exactly equal to the total, you are measuring the
total.**

So the function was removed rather than published or left behind an
`#[allow(dead_code)]`. **A reader that contradicts a known-good source by an
order of magnitude does not get shipped because its formula looks right.** If
someone wants a system-wide figure in `silicon`, take it from `core::cpu`, which
is correct, rather than re-deriving it here.

One earlier note in this session, that the reader was simply "wrong", was
itself premature: the first probe showed `idle delta = 0`, and that was real —
cargo was compiling at the time and the machine genuinely was saturated. The
100% on a quiet machine is the finding; the first reading was not.

### A warning in a doc comment does not bind the caller

The best-documented defect in the crate, and the documentation did not help.
`silicon::apple::estimate_link_speed` carried this:

> These are guesses keyed off the interface name, not measurements: `en0` is
> assumed to be Wi-Fi 6 and anything else `en*`/`bridge*` gigabit Ethernet.
> **Callers must not present the result as a read link rate.**

Two lines below, the caller assigned it straight to `link_speed_mbps`. So a Mac
reported a 1200 Mbps link for `en0` because the name starts with "en" — `en0` is
not always Wi-Fi, and a 10GbE port is not the 1000 the next branch would give
it.

**A warning in a doc comment does not bind the caller; a type does.**
`link_speed_mbps` is `Option<u32>` now and the function is gone. The same field
was zero on Linux for a down interface and for the whole wireless path (*"Would
need iwconfig/nl80211"*), and on Windows whenever `CurrentBandwidth` was absent.
All `None`.

That is the fourth kind of marker for this defect family, and the one that
should be least reassuring:

1. A comment admitting the code cannot determine something — the original grep.
2. A consumer guarding `> 0` to undo a sentinel its source made.
3. A field whose only purpose is to be a fallback (`base_frequency_mhz`).
4. **A doc comment correctly forbidding exactly what the caller does.**

Prose next to the code cannot enforce anything. Everything in this session that
actually held — the `Option` migrations, the conformance suites, the
`source_hygiene` and `plausibility` guards — works because it fails a build or a
test, not because it tells the next reader what to avoid.

### Guesses admitted in comments, published as readings

The `Win32_PnPEntity` row of the map above led to `silicon`'s NPU support, where
every field that was not read carried a number anyway — and **each one had a
comment admitting it**:

```rust
let cores = if vendor == "Intel" { Some(16) }   // Intel NPU ~16 compute units
            else if vendor == "Qualcomm" { Some(8) } else { None };
utilization: 0,                    // NPU utilization requires vendor-specific APIs
cores: Some(16),                   // Most Apple Silicon has 16-core ANE
cores: Some(128),                  // Typical TPU core count
utilization: 0,                    // Would need TPU API
```

"~", "Most", "Typical", and two "would need an API we do not call". This is the
handoff's own grep — *a comment admitting the code cannot determine something,
sitting beside a returned value* — with five hits in one module.

The core counts are worse than they look: they key off `vendor`, and `vendor` is
itself a **substring match on the device name** when the manufacturer string is
absent. A guess keyed off a guess. All are `None` now, and
`NpuInfo::utilization` is `Option<u8>`.

`read_npu_utilization` on Linux had two zero paths, and the second is the more
interesting: unreadable sysfs returned `0`, and so did the **first** call, under
`// First reading — store baseline, return 0`. It is a delta reader with no
previous sample — it cannot know the utilization yet, and reported 0% for every
NPU on the first tick after startup. **The same defect as an intrusion baseline
reporting "clean" on its first run**, which was `2360068`, the commit this
session opened with.

`AcceleratorInfo::utilization` in the TUI went to `Option<f32>` with it, so the
absence survives to the screen: a device with no utilization counter shows a
dash rather than an idle-looking 0% gauge.

### The API map, and the fifth reader it found

Acting on the rule below — grep for the API, not the reader — produced a map
worth keeping. Every Windows API and WMI class read from more than one module:

| Read from | Modules |
|---|---|
| `CallNtPowerInformation` | platform, silicon *(both now fixed; the other three references are prose)* |
| `Win32_VideoController` | codec, display, drm_monitor, gpu, hardware_ai, motherboard, platform |
| `Win32_PnPEntity` | audio, bluetooth, camera, motherboard, pci_devices, pcie, sensors, silicon, usb |
| `Win32_Processor` | cpu_microarch, crypto_accel, hardware_ai, interconnect, motherboard, numa, silicon |
| `Get-PhysicalDisk` | firmware, hardware_ai, smart, storage_controller |
| `Win32_OperatingSystem` | boot_config, hardware_ai, motherboard, numa, os_info |

**Seven modules read `Win32_VideoController`, and `AdapterRAM` is 32 bits.**
`7607401` fixed the 4GB cap in `hardware_ai` and `platform::windows`; `gpu/amd`
and `gpu/intel` prefer DXGI and fall back to it. **`drm_monitor` read it raw** —
no DXGI, no registry, no comment — and reported `vram_total_bytes: Some(4293918720)`
for a 24GB card. It has no internal callers and is re-exported from `lib.rs`, so
the only people it lied to were library users, which is why running the binary
never caught it.

Fixed by preferring the registry `REG_QWORD` and withholding anything at or
above the cap rather than publishing ~4GB. `examples/drm_vram_check.rs` confirms
it against `nvidia-smi`: **24.0 GiB** for each 3090 Ti, 2.0 GiB for the
integrated Radeon.

**A module with no internal callers is not dead — it is public API, and it is
the one place running the binary cannot check.** Four of five readers of this
field were fixed because they were reachable from a command.

Checked and clean in the same sweep: `silicon/windows.rs` appears in the
`Win32_Processor` list but reads no `Name` from it, so the space-padding defect
fixed in `hardware_ai` has no sibling there.

### Two readers, one API, one lie — and only one was fixed

Following the lesson below immediately paid. Sweeping for `unwrap_or` on the
fields made `Option` this session turned up
`src/silicon/linux.rs`'s `read_cpu_frequency(cpu_id).unwrap_or(0)` — and pulling
that thread found something larger on Windows.

`silicon::windows::read_cpu_frequencies` calls
**`CallNtPowerInformation(ProcessorInformation)`** — the same API, on the same
machine, as `platform::windows::get_cpu_frequency`, which `6617020` fixed
earlier today for returning the nominal clock rather than the current one. This
second reader was never touched, so `cargo run --example cpu_monitor` printed:

```
Core  0 [P]: 4400 MHz,   0% util
Core  1 [P]: 4400 MHz,   0% util          ... for all 24
  Average Frequency: 4400 MHz
```

4400 on every core of a 9900X, whatever it is doing. **The module is live**: the
TUI's silicon pane uses it on all three platforms.

Worse than the duplicate: when the call *failed*, the reader filled the vector
with `self.base_frequency_mhz`, publishing a registry nominal figure as every
core's measured current one. Removing that fallback made `base_frequency_mhz`
and `detect_base_frequency` dead — **the field existed only to fabricate**,
which is a useful signal in itself. Both deleted.

`CpuCore::frequency_mhz` and `CpuCluster::frequency_mhz` are `Option<u32>` now,
and cluster averages go through `average_reported_mhz`, which returns `None`
rather than dividing by a count that includes cores contributing zero. Averaging
unread cores as zero drags the figure toward zero in proportion to how much was
unreadable and reports the result as a measurement.

**The sweep that found it is worth repeating after any `Option` migration:**

```bash
grep -rnE "(the_field_you_changed)[^;]*unwrap_or" src/ examples/
```

**And the generalisation: when a reader is wrong, grep for the API it calls, not
for the reader's name.** `get_cpu_frequency` and `read_cpu_frequencies` share no
identifier; they share `CallNtPowerInformation`. Two modules can hold the same
defect and no rename-level search will connect them.

### A fix in this session introduced the defect it was fixing

`simon cli cpu` printed **"Clock: 0 MHz (max 4400 MHz)"**. The zero came from
`6617020`, earlier in this same session — the fix that stopped Windows reporting
the nominal clock as the current one. Its code said:

```rust
// Zero is what the resolver already reads as "not measured" here, the
// same way `min` uses it below.
current: if entry.current_mhz == entry.max_mhz { 0 } else { entry.current_mhz },
```

The comment is true and the reasoning was wrong. **The ontology resolver did
read that zero correctly. Nothing else did**: `simon cli cpu` printed it, the
agent surface published `"frequency_mhz": 0` from four call sites, the GUI
exported a `cpu_frequency,0,MHz` CSV row, and `http_server` recorded a
`simon_cpu_frequency_mhz 0` Prometheus sample.

**Checking one consumer is not checking the consumers.** That is the same
mistake as every "reader wired into one consumer" finding in this file, made
while fixing one of them, and the fields should have been `Option` from the
start — which is what they are now.

**`tests/plausibility.rs` was already warning about it and the assertion was too
weak to fire:**

```rust
assert!(freq.current > 0 || freq.max > 0,
    "... absence must be `None`, not zero");
```

The rule was right, the message named the exact failure, and the `||` let a zero
`current` through whenever `max` was known — which is every Windows machine. It
now checks each field separately, and `Some(0)` fails for any of the three.

Alongside it, `simon cli cpu` said **"Cores: 24"** one line under "AMD Ryzen 9
9900X 12-Core Processor". That is `cores.len()`, the logical count — the same
contradiction fixed in `simon status` at `425ff4a`, still present in two other
printers because that fix touched `fetch::summary` and nothing else. Both say
"Threads" now.

**Both halves of this entry are the same lesson**: a fix applied where the
defect was seen, rather than where the defect is.

### One more "none" that meant "cannot look"

`simon cli engines` printed **"No engines detected"** on Windows. Its own
`--help` says the command reads `/sys/kernel/debug/clk` and is Linux-only, and
`read_engine_stats` on Windows is:

```rust
// Windows doesn't have the same engine concept as Jetson
// Return empty stats
Ok(EngineStats::default())
```

An empty success, under a comment that knows exactly why. The help text was
honest and the output was not, which is the worse way round: a user who runs the
command without reading `--help` first is told their machine has no engines.

The reader cannot return an error, because `Simon::snapshot` requires every
reader to succeed and that would take the whole Windows snapshot down — so the
message is cfg-gated at the display instead, the same shape as `FREQUENCY_ABSENT`
in the resolver. **Tenth instance of this family**, after the nine in `ebab956`.

### Surfaces checked this round that are sound

Recording these so the next reader does not re-derive them. Each was checked by
running it and confirming against a second source, not by reading the code:

- **`simon daemon`** enforces what it claims. `host = "0.0.0.0"` without an
  `api_key` refuses to start, naming the reason; loopback starts and prints
  "Authentication: anonymous (loopback only)". Its sample config marks the fleet
  section *"NOTE: not implemented. These keys parse, but nothing pushes"*.
- **`simon profile active`** reports "311 of 503 processes have a known NVIDIA
  driver profile", including `svchost.exe` and AMD's own `amdow.exe`, which
  looks wrong and is not: scanning `nvdrsdb*.bin` independently yields 11,521
  names and all three are genuinely in NVIDIA's database. NVIDIA ships profiles
  for system executables.
- **`simon ai manifest`** names a model per vendor but says in `model_discovery`
  that the field is "only a starting suggestion" and points at the live endpoint
  — the right handling for a value that goes stale.
- **`simon get`** returns `unavailable` with the reason for an absent entity and
  names unknown ids as unknown, with a search suggestion.
- **`simon profile show cpu`** states that XTU-style MSR access needs a signed
  kernel driver and is not implemented, rather than showing an empty table.

### A denominator of two, reported as a clean bill of health

`simon profile deviations` printed:

> No settings deviate from their declared defaults.

It had compared **two settings**. `simon profile list` reports 23,541 across the
five providers — the GPU one contributes 23,445 — and `deviations_from_default`
skips anything with no declared `default`:

```rust
let Some(default) = s.default.clone() else { continue };
```

Two of 23,541 declare one. **0.0085%.** The sentence a user reads is a claim
about their machine; the fact behind it was "the two settings that could be
compared both matched, and nothing is known about the other 23,539".

This is the crate's own defect in a place with no hardware in it: **an absence
converted into a reassuring conclusion.** It is close kin to the nine readers
that said "none" where they meant "cannot look", except the absence here is a
missing *reference value* rather than a missing reading, and the conclusion is
drawn by arithmetic rather than by a reader.

`deviation_report` now carries a `DeviationCoverage` beside the list, so the
denominator reaches the JSON caller too, and the command says:

```
None of the 2 setting(s) that declare a default differs from it.
  23539 of 23541 settings declare no default, so this says nothing about them.
```

**The generalisable check: when a report filters its input, ask what fraction
survived the filter, and whether the summary sentence says so.** An empty result
after a 99.99% filter is not the same statement as an empty result over
everything, and only one of the two is what the words claim.

### A control that reports success without acting

`simon cli audio` printed **"Master Volume: 100%"** and **"Muted: No"**. Both
are constructor defaults. No `refresh_*` path on any platform assigns either
field — the only writes were the setters — so those two lines said the same
thing on every machine simon has ever run on, and the agent tool surface
published `"master_volume": 100, "is_muted": false` alongside them.

`src/audio/mod.rs` was checked in an earlier sweep and recorded here as clean,
on the grounds that its one "placeholder" comment was in a test. **That check
looked at comments and did not run the command.** The constant is in the
constructor, where nothing marks it.

Then the setters, which are worse:

```rust
pub fn set_master_volume(&mut self, volume: u8) -> Result<(), SimonError> {
    ...
    self.master_volume = Some(volume);
    Ok(())
}
```

All four of them — master volume, master mute, per-device volume, per-device
mute — assigned a field and returned `Ok(())`, touching no audio API on any
platform. **A caller set the volume to 20, was told it worked, and the machine
did not change** — and `master_volume()` then returned the 20 it had just
stored, so reading it back *confirmed* the lie. `examples/hardware_control.rs`
demonstrates exactly this, and printed "New master volume: Some(75)".

They return `NotImplemented` now. **A control that reports success without
acting is worse than one that is absent**, because nothing downstream can tell.
This is the `--offline` defect from `agent/mod.rs` in a second place: a flag or
a setter that "enforced nothing", where the only evidence was that no caller
existed for the thing it should have driven.

Two more of the familiar shapes in the same file: the default audio device was
given `volume: Some(100)` while every other device got `None` — so the one
device a user looks at carried the invented figure — and a **fallback invented
device**, "Default Audio Output", active and unmuted at 100%, pushed when the
enumeration found nothing. That is the Intel root hub from `cc7aac6` again, one
module over. Both gone.

**Generalised, in the commit after this one.** `simon cli usb` and `simon cli
audio` both invented a device when enumeration failed, and both were outside
what `tests/plausibility.rs` covered: that suite guarded synthetic *displays*
because displays are where the pattern was first caught, and the rule was never
widened. `readers_that_find_nothing_invent_nothing` now covers all three.

It asserts the **signatures** rather than emptiness, because a real machine does
have audio and USB devices; what it must never have is the Intel root hub at
`8086:0001` or an endpoint called "Default Audio Output". Confirmed by putting
the invented default-device volume back, which fails it with *"AMD High
Definition Audio Device: no platform reads a device volume, so none may report
one"*.

**And a test can pin the defect as the contract.** Removing the invented audio
device broke `test_audio_monitor_devices` on the Linux and Windows runners. That
test asserted `!monitor.devices().is_empty()` under the comment *"Should have at
least one device (placeholder on all platforms)"* — **it existed to assert that
the placeholder was present**, and it passed here only because this desktop has
real audio hardware. A headless runner genuinely has none.

The honest assertion is about the shape of whatever is reported — every device
has an id and a name — not about a count the hardware decides. Nothing else in
the suite makes that mistake: every other `is_empty` assertion is per-item.

**The lesson underneath is about the shape of a guard.** A test written for the
module where a defect was found will not catch the same defect one module over,
and this family has now appeared in three. When a suite exists for a *pattern*,
name the pattern in it.

### A GPU context is not a GPU workload

`simon cli processes` reported **24 processes under "GPU Compute"**. The top
five were `WindowsTerminal.exe`, `brave.exe`, `Code.exe` and a Logitech settings
agent. `nvidia-smi` labels every one of them `C+G` with `[N/A]` memory: they are
drawing windows.

`ProcessCategory::classify` took `is_gpu_process: bool`, which is
`!gpu_indices.is_empty()` — *has any GPU association at all* — and that branch
ran **first**, ending in `return Self::GpuCompute` for anything not matching an
AI/ML or game name. On Windows every windowed application holds a GPU context,
so the branch swallowed the entire desktop before the name-based categories
could see it.

This is the `ai_workload` PyTorch defect again — an **identity** inferred from a
GPU association — and the same remedy applies: keep the branches backed by
evidence (a GPU context *plus* a python or steam name is a real signal) and let
everything else be classified by what it is. A browser is now a browser.

`GpuCompute` survives, for a GPU-holding process that matches nothing by name,
but only after every other category has had its turn, and its label is now
**"Using GPU"** rather than "GPU Compute". On this desktop it holds fourteen:
`iwx.exe`, `RadeonSoftware.exe`, two NVIDIA overlays, the Logitech agent. That
is a true statement about all of them; "GPU Compute" was not.

**`GpuProcess` already carries `process_type`**, and the NVIDIA reader sets
`Graphics` or `Compute` correctly. The categoriser never looked at it — it took
a bool derived from the index list instead. Worth knowing before relying on it,
though: under WDDM, NVML's compute list includes plain graphics apps
(`nvidia-smi --query-compute-apps` here returns `explorer.exe`), so on Windows
that field cannot settle compute-versus-graphics either. The fix does not lean
on it.

**Fixed in the commit after this one.** `GPU(MB)` printed `0.0` for every
GPU-attributed process on this machine, and `nvidia-smi` reports `[N/A]` for all
of them — WDDM does not expose per-process GPU memory to NVML. The sentinel was
made at `process_monitor.rs`'s attribution loop:

```rust
let gpu_mem = gpu_proc.memory_usage.unwrap_or(0);   // Option<u64> -> 0
proc_info.total_gpu_memory_bytes += gpu_mem;
```

`GpuProcess::memory_usage` is already `Option<u64>` and honest;
`ProcessMonitorInfo::total_gpu_memory_bytes` is `u64` and has nowhere to put the
absence — the same shape as `SystemSnapshot`'s GPU columns and `DisplayInfo`'s
mode. **`observability/api.rs` already compensates** with
`if total_gpu_memory_bytes > 0 { Some(..) } else { None }`, which is the fourth
consumer this sweep found guarding a sentinel its source should not have made.
The field is `Option<u64>` now. It had 45 references across the GUI, TUI, agent
surface and examples, which is why it was a separate commit from the classifier.

**Three of the five consumers were already compensating** — `observability/api.rs`
with `> 0`, and the TUI in two places, printing a dash where the CLI printed
`0.0`. That ratio is the tell for this whole family: when most consumers guard a
value, the guard belongs in the type, not in each of them.

`ProcessSnapshot::gpu_memory_bytes` went with it, so `DB_VERSION` is **3**. The
time-series database was persisting the same zero, which is the defect the
version-2 bump existed to remove — one field further down, and missed the first
time because the fix stopped at the columns it was looking at.

### An identifier is a bad place for a sentinel

`simon cli usb` printed **`[0000:0000]`** for four of forty-one entries — every
root hub and a "USB4 Virtual power coordination device". `Get-PnpDevice -Class
USB` shows why: a root hub's id is `USB\ROOT_HUB30\9&EBA7CE&0&0`, with no
`VID_` or `PID_` in it at all. `parse_vid_pid` returned `(0, 0)` for that, and
`0000:0000` is a value a device could legitimately report, so nothing downstream
could tell the two apart.

**One of the two callers already guarded `if vid != 0 || pid != 0`**, and the
TUI pane guarded it again before printing. So the sentinel was known to be
meaningless in two places out of four, and the CLI and the agent surface — which
published `"vendor_id": "0000"` — did not. Third time this sweep that one
consumer knew and another did not.

The zero also had to be *re-guessed* by the ontology resolver, which published
the identifier `"0000"` as `measured`. It now says
*"this entry carries no USB vendor id; root hubs and virtual devices have none"*.

The regression test keeps the distinction explicit, including the case that
makes it matter: `VID_0000&PID_0000` parses to `Some(0)`, not `None`. **A device
reporting zero and a device reporting nothing are different facts**, and a
sentinel in an identifier field is precisely the shape that cannot express both.

**And a second defect in the same file, worse than the first.** When the Linux
sysfs walk found nothing, the reader invented a device:

```rust
// Fallback
if self.devices.is_empty() {
    self.devices.push(UsbDevice {
        vendor_id: 0x8086, product_id: 0x0001,
        product: Some("USB Root Hub".to_string()),
        speed: UsbSpeed::High, ...
```

An Intel root hub, at high speed, read from nothing. A machine whose USB tree
could not be enumerated reported one device that does not exist, and no caller
could distinguish that from a machine with exactly one hub. Deleted; an empty
list is the honest answer. This is the same shape as the synthetic placeholder
displays `tests/plausibility.rs` already guards against — worth knowing that
family has another member, in a module that suite does not cover.

### A gamepad counted as a radio, because its name contains "Controller"

`simon cli bluetooth` reported **"Adapters: 2, Devices: 9"** on a machine with
one radio and two paired peripherals. `Get-PnpDevice -Class Bluetooth` settles it
in one command, which is how it was checked.

The classification was a regex over the *display name*, inside a PowerShell
string:

```powershell
Where-Object { $_.PNPClass -eq 'Bluetooth' -and $_.Name -match 'Radio|Adapter|Controller' }
```

An **Xbox Wireless Controller** matches `Controller`, so a gamepad was reported
as one of the machine's Bluetooth radios — and the device query excluded the
same three words, so it then vanished from the device list. The device filter's
`-notmatch` also admitted "Generic Access Profile", "Device Information
Service", "Bluetooth LE Generic Attribute Service" and "Bluetooth Device
(RFCOMM Protocol TDI)": six GATT services and a protocol driver, counted as
things the user had paired.

**The `PNPDeviceID` was in the same query object all along and separates them
without ambiguity** — `USB\` is the radio, `BTHENUM\DEV_`/`BTHLE\DEV_` is a
paired peripheral, `BTHLEDEVICE\{guid}` is a GATT service on one, `BTH\MS_` is
the stack. This is the handoff's own *guess placed in front of a reading*, with
the reading sitting in an adjacent field.

**The classifier moved from the PowerShell string into Rust**, which is the part
worth keeping. Logic inside a shell string cannot be unit-tested at all; the
same logic in Rust is tested against all eleven Bluetooth-class entries from
this desktop, verbatim, asserting one adapter and two peripherals. Ordering
matters and is now covered: `BTHLEDEVICE` also begins with `BTHLE`, so a GATT
service classifies as a peripheral if the peripheral check runs first.

### A display's mode had nowhere to be absent

`simon cli display` printed **`RX-A740 0x0 @ 0Hz Dvi`** — a Yamaha AV receiver
on HDMI, attached and named, whose current mode Windows will not report. The
ontology got this right and said so in a reason that reads, in as many words,
*"zero is not a resolution"*. Five other consumers printed the zero anyway.

`DisplayInfo` had `width: u32`, `height: u32`, `refresh_rate: f32` — the same
shape as `SystemSnapshot`'s GPU columns two commits earlier, and the same
consequence: **an absence with nowhere to go becomes a zero, and the resolver
downstream has to reconstruct it from a sentinel.** The doc comment on
`aspect_ratio` already named the diagnosis exactly — *"a return type with no way
to say nothing"* — because the sibling defect was fixed there this session. That
fix stopped at the method and left the fields.

Fixing the fields found consumers a grep did not:

- `ai_api/tools.rs` published `"resolution": "0x0"` to agents, twice.
- `tui/app.rs`, which the grep missed entirely — the compiler found it.
- `profile/display.rs` published a `0 Hz` refresh-rate row.
- `examples/display_monitor.rs` summed `0 * 0` into a total-pixels figure.

And it turned up a defect invisible on this machine: **the Linux and macOS
readers write `refresh_rate: 0.0` unconditionally**, because neither parses a
rate. Every display on both platforms reported 0 Hz as though measured.

**The zero came from the data source, not from a missing key**, which the first
attempt got wrong: `item.get("Width")` returns `Some(0)`, so making the field
`Option` left `Some(0)` and the output unchanged. Normalising is now done once
at the reader — a display is not zero pixels wide, so zero becomes `None` there
rather than at each of the five consumers.

### The command named for the reading was the one not doing it

`simon cli temperature` printed **"No temperature sensors detected"**. Seconds
later, in the same binary, `simon snapshot` read two GPU temperatures and three
NVMe drive temperatures, all `measured`. `--format json` returned
`{"sensors": {}}`.

The Windows reader behind it looks in three places — OpenHardwareMonitor's WMI
namespace, LibreHardwareMonitor's, and CIMV2 thermal-zone counters — and none is
present on a stock desktop. That part is correct and the resulting empty map is
correct. **What was wrong is the sentence.** "No temperature sensors detected"
is a claim about the machine; the truth was a claim about the reader.

It also contains, in as many words, the reason the command was incomplete:

```rust
} else {
    continue; // Skip other sensors (GPU temps come from NVML)
}
```

That comment is true of the *crate* and false of the *command*. GPU temperatures
do come from NVML — and nothing in `simon cli temperature` ever asked NVML.

`simon cli power` was the same defect exactly: `PowerStats.rails` is hwmon,
which Windows does not expose, so it printed "No power rails exposed by this
platform" while the ontology held both GPUs' draw and limit, the battery
percentage and the three power profiles. Someone had already fixed the
neighbouring half of it — there is a comment explaining why "Total Power: 0.00W"
was removed — and stopped at the rails.

Both now read `ontology::resolve::snapshot()`, which is the fifth and sixth
consumer to adopt a reader that was already right. **The selection lives in
`fetch::{temperatures,power}` rather than in `main.rs`, so it can be tested**,
and `temperatures` keys on the declared unit rather than the id shape, so a
temperature entity added anywhere is shown without touching it.

What the fixed command prints is the argument for the whole approach: ten
readable temperatures, and six unreadable ones each with its own reason —
including the two that explain the original symptom truthfully, *"on Windows
most board sensors require a signed kernel driver"*. The old code had that fact
and threw it away.

**The regression test asserts the selection spans more than one subsystem
prefix.** Narrowing it back to `thermal.*` fails with
`every temperature came from one subsystem ({"thermal"})`, which is the defect
verbatim.

**The first version of it was wrong, and CI said so on all three runners.** It
resolved `resolve::snapshot()` and I described it as hardware-independent
"because every temperature entity is declared whether or not a host can read
it". The declarations are; a *snapshot* is not. Instanced entities produce no
row at all when the instance is missing, so a runner with no GPU and no NVMe
drive resolves nothing the test was looking for. It passed on this desktop for
the same reason the defect it guards existed: the machine had the hardware.

The fix is to build the readings from `Ontology::build().entities` — one
`Reading` per declared entity, unit carried, value `None` — and run the
predicate over that. The test now takes 0.00s instead of 37s, because it never
touches hardware, and it means what I claimed the first version meant.

### CI caught a test I wrote, on prose I added after the gate

`documented_http_paths_match_the_route_table` failed on all three runners at
`708ae50`, on **HANDOFF.md** — this file, which quotes the wrong Prometheus path
three times while explaining that it is wrong. The test cannot tell a
counter-example from a recommendation.

The exemption is easy (this file is excluded from the published crate and
instructs nobody, so a mention here is a citation). **The process failure is the
part worth keeping.** The order was: fix, add test, confirm it fails on the old
text, restore, run the gate green, *then write the handoff entry*, then push.
The entry introduced the failure and was never tested, because the gate had
already been run.

**This file is an input to the test suite now** — `documentation_links` and
`source_hygiene` both read it. Editing it after the gate invalidates the gate,
in exactly the way that editing any source file would. Run the tests last, not
the prose.

That is the second time this pattern has appeared in three commits: at `7c79f8b`
a local gate that could not see feature-gated code, and here a local gate run
before the last edit. Both times the gate was correct and the sequencing was
not.

### The contradiction was on screen, two lines apart

`simon status` printed:

```
CPU        AMD Ryzen 9 9900X 12-Core Processor
Cores      24
```

`fetch::summary` labelled `cpu.cores.logical` as "Cores". The reading was right
and the label was wrong — `Win32_Processor` confirms 12 cores, 24 logical
processors — and **both counts are already measured entities**, so the fix was
to name each one rather than to read anything new. It now prints `Cores 12` and
`Threads 24`.

**This is the cheapest defect in the whole file to have found, and it survived
six major versions.** Nothing about it needed hardware knowledge or a second
tool: the name of the part and the wrong count were two lines apart, in the
output of the command most likely to be run first. It is the strongest argument
for scan #1 there is — grep cannot see it, types cannot see it, and no test that
asserts a summary *is produced* can see it.

Alongside it, `as_bytes` divided by 1024 and labelled the result GB, so 100.5 GB
of installed memory printed as "93.6 GB". 93.6 is the correct number and GB is
the wrong name for it — that quantity is 93.6 GiB. A crate that carries QUDT
units through its ontology should not lose them in the line most people read.
Now GiB.

**Deliberately not fixed, and worth stating plainly rather than leaving
implied:** the same 1024-with-decimal-labels convention runs through five other
byte formatters (`bin/main.rs::format_bytes_short`, `gui/app.rs::format_bytes`,
`memory_management::{format_bytes,format_size}`, `tsdb::format_size`) and a long
tail of internal constants and test fixtures. Only `fetch::as_bytes` reaches
`simon status`, which is the surface that was read; the rest is a crate-wide
cosmetic sweep touching the GUI, which this handoff has repeatedly deprioritised.
It is a real inconsistency and it is still there.

### A constant read without its prefix, documented as a route

`simon serve --help` said: *"Prometheus metrics are at /metrics/prometheus (not
/metrics — that route returns JSON)."* Both halves are false. `curl` returns 404
for `/metrics/prometheus` and 404 for `/metrics`; the served path is
`/api/v1/metrics/prometheus`, which the server's own startup banner and
`grafana/README.md` both state correctly.

The mechanism will recur, so it is worth naming: `routes::METRICS_PROMETHEUS` is
the string `"/metrics/prometheus"`, and the dispatcher compares it only *after*
stripping the `/api/v1` prefix. Reading the constant and documenting it verbatim
yields a path that does not exist, and nothing about the constant warns you.

**A help string is the one claim a user cannot check before acting on it.** They
run what it names and get nothing back. `documented_http_paths_match_the_route_
table` in `tests/documentation_links.rs` now composes the path from the route
constants and fails on any mention that drops the prefix; it was confirmed
against the old text first.

Two things looked wrong here and are not, which is worth recording so the next
reader does not spend the time twice:

- `simon_gpu_{0,1}_fan_speed_percent 0` at 49 °C and 37 °C is a **true zero** —
  `nvidia-smi --query-gpu=fan.speed` reports `0 %` for both. Zero-RPM idle on a
  3090 Ti. Checked in a second tool rather than assumed, per the rule below.
- The Prometheus exporter omits `simon_gpu_2_temperature_celsius` entirely
  rather than publishing a zero, which is correct for Prometheus and is the
  behaviour the recorder lacked.

**Open, not fixed:** the OpenAPI document at `/api/v1/openapi.json` advertises
three paths (`/health`, `/api/v1/context`, `/api/v1/gpus`) out of roughly twenty
that the dispatcher serves. Everything it advertises works, so it states nothing
false — but an agent handed that spec would conclude simon serves three
endpoints. Four route constants (`METRICS`, `STREAM`, `EVENTS_SUBSCRIBE`,
`PROCESS_BY_ID`) are declared and never dispatched; they are unreferenced dead
constants rather than advertised lies, which is why they were left alone.

### A privacy screen is a reading too, and this one was wrong three ways

`simon privacy` was the last unread surface. It has no hardware in it, which is
why it went unexamined for so long, and it turned out to hold the most confident
false statements in the crate.

**There is no telemetry in simon.** No endpoint, no collector, no serialiser.
`ConsentScope` is referenced only by the `privacy` subcommand that displays it;
`should_collect_data` has no callers at all outside its own module; every
outbound `.post` in the crate goes to an LLM backend the user configured. In
front of that emptiness sat five categories, twenty itemised data points
("Crash stack traces (anonymized)", "Frame time distributions"), an opt-in flow,
and a persisted config with timestamps — and `simon privacy opt-in` answered
"All data collection categories have been enabled."

**The direction of a false claim does not decide how serious it is.** Every
defect before this one overstated what simon knew; this one understated what it
did, which reads as reassuring and is therefore harder to doubt. It is still a
confident wrong answer about the program's own behaviour, in the one area a
user cannot check for themselves, and it degrades: the disclosure is already
written and already consented to on the day someone adds a collector.

Three defects, not one:

1. **The apparatus does not exist.** Fixed by saying so on every screen, and by
   `ConsentScope::is_collected`, which returns `false` for all five and is the
   single place to change. `should_collect_data` consults it, so consent to a
   category nothing implements cannot report that it should collect.
2. **`has_consent` defaulted to granted**, under a comment reading "opt-in by
   default" and a module header listing "Opt-in by default" as a principle. A
   default of granted is opt-out; the two cannot both be true. Nothing consumed
   the answer, so it was inert — and it would have started collecting from
   everyone who never answered on the day it stopped being inert. Now `false`.
3. **The module claimed compliance with GDPR and CCPA.** A library is not in a
   position to make that claim about a deployment, and default-granted consent
   is the specific thing the first of those forbids. Removed rather than
   softened.

The global `--no-telemetry` and `--offline` help lines said simon did "telemetry,
analytics, or crash reports" on **every subcommand's `--help`**, which is the
widest-reach false statement the crate had. `--offline`'s one real effect is
worth stating accurately: `agent::refuse_if_offline_and_offhost` declines a
backend that would relay the question to a vendor. That check is good, and its
doc comment records that the flag previously enforced nothing at all.

### `$_` is not a debugger, and it cost the privacy screen its credibility

Pulling the thread on why `opt-in` said "enabled" and `status` said "DENIED" on
the next line landed on `sandbox.rs`:

```rust
let debugger_vars = ["_", "LLDB_DEBUGSERVER_PATH", "GDB", "PYTHONBREAKPOINT"];
```

`$_` is set by **every POSIX shell** to the last argument of the previous
command. So every run started from bash, zsh or sh set `is_debugged`, therefore
`is_sandboxed()`, therefore `has_consent() == false` for every scope — silently,
because the display folded three unrelated causes into one word. `simon
privacy status` was reporting a shell variable as the user's privacy choice.
`cargo run --example sandbox_demo` printed **"Sandboxed: Debugger Attached"**,
"Protection Status: ACTIVE" and "Network Transmission: BLOCKED" on a bare-metal
desktop.

The other three names in that list say nothing about *this* process either: gdb
does not export `GDB`, and `PYTHONBREAKPOINT` configures a Python hook in a Rust
binary. The two real checks — `TracerPid` on Linux, `IsDebuggerPresent` on
Windows — were already there and already correct, sitting directly above. **The
heuristic was pure downside: it could only add false positives to checks that
did not need help.**

Two more in the same file, both confirmed rather than reasoned:

- The Windows Hyper-V check read `C:\Windows\System32\drivers\vmbus.sys` and
  set `is_vm` if the contents were non-empty. The file **ships with every modern
  Windows install**, host or guest, so its presence proves nothing — and it is a
  binary driver, so `read_to_string` fails on invalid UTF-8 and the check
  **never ran at all**. Both checked here: present, 210 KB, invalid UTF-8 at
  byte 2. It now calls `virtualization::detect`, which 3.10.0 already built
  correctly out of CPUID leaf 0x40000003 — *a second consumer that never
  adopted the good reader, found four sessions after the pattern was named.*
- Linux `sys_vendor` matched `"Microsoft Corporation"`, which is the sys_vendor
  of every Surface. Bare metal reported as a VM.

**The regression test was confirmed against the old code before being believed**,
per the rule below, and failed with exactly the right message:
`reported a debugger with none attached; indicators: ["Debugger env var _ detected"]`.

### The recurring string mangling, finally diagnosed and now caught

Three sessions have lost `\` line continuations inside Rust string literals,
leaving runs of source indentation embedded in user-facing text, and the handoff
has recorded it twice as unexplained. The mechanism is now clear enough to state:
**a `\` at the end of a line does not survive being written through a shell
heredoc**, so a literal authored that way arrives collapsed. It still compiles,
still lints clean, and every assertion about it still passes, because all the
words are present.

`tests/source_hygiene.rs` now fails on the signature — six or more spaces
between two lowercase letters, inside a line containing a quote, outside a
comment. It found **twelve** instances on its first run, five of them already
committed, including one shipped in `4846c3f` in an EDID setting description.

The fix is always `concat!("first part ", "second part")`, which has no
continuation to lose. That was already the crate's convention; this is why it
has to be.

### The reader was honest; the consumer threw it away

`gpu.2.thermal.temperature` resolves **"unavailable — no temperature sensor
exposed for this adapter"** in `simon snapshot`, and the Prometheus exporter
omits the series entirely, which is correct for Prometheus. The same
`Option<u32>`, on the same tick, was written into the time-series database as
`0.0` and printed by `simon record query` as `Temp: 0°C` — an integrated AMD
adapter reading as the coolest device in a box holding two 3090 Tis at 44 and
38 degrees.

**Three consumers of one already-honest reader; two respected the `Option` and
the third discarded it at the type boundary.** This is the "reader wired into
one consumer" pattern further down, in its most treatable form: nothing needed
to be measured differently, only carried. `SystemSnapshot`'s four per-GPU
columns were `Vec<f32>`/`Vec<u64>`, so there was nowhere for an absence to go,
and `unwrap_or(0)` was the only way to satisfy the type.

**A `Vec<T>` whose elements come from `Option<T>` readings is the shape to look
for.** The lossy conversion is forced by the declaration, so it never looks like
a decision anyone made. Grepping `unwrap_or(0)` does find it, among 884 others;
what singles this one out is that the value crosses into **storage**, where it
is exported to JSON, averaged, and read back later by someone who cannot ask
what the zero meant.

Worse than the sensor: a device whose query *failed on that tick* was recorded
as `0%` utilisation, `0` bytes, `0°C`, `0 mW` — a complete, plausible reading of
an idle, cold, unpowered GPU. The comment directly above that code said
"carries None for a device whose query failed this tick", and the next four
lines turned it into zeros. Four per-process columns did the same, with comments
admitting they were never tracked at all.

The fix makes the columns `Vec<Option<_>>` and bumps `DB_VERSION` to 2.
**Version 1 files are now rejected rather than migrated**, and the reason says
why: a stored zero does not record whether it was measured, so any conversion
would have to guess, reintroducing exactly what was removed. The rejection names
`simon record clear` as the recovery, and that command was checked against a
real version-1 file — it deletes without opening.

### Three scans, in the order they pay

Each found things the one before it could not, and all three are cheap:

1. **Read the output of anything that produces conclusions.** `hardware_ai`
   asserted "No SSD detected" at 0.85 confidence on a machine with three NVMe
   drives, and classified a tower as a GamingLaptop.
2. **Group readings by entity family and flag a family whose instances are all
   identical.** That is what a broken per-device reader looks like from outside.
   It found `cpu.core.{n}.frequency` reading 4400 on all 24 cores,
   `disk.controller.{n}.ports` hardcoded to 1, and
   `system.printer.{n}.color` false for every printer.
3. **Take two snapshots seconds apart and list the numbers that did not move.**
   Most are legitimately static; the interesting ones are the volatile-sounding
   survivors. That found the NVMe SMART offsets — a drive reporting 2189 power
   cycles against 43 power-on hours, a cycle every seventy seconds.

Then, once a parser is implicated, **audit the crate's other offset-based
parsers**, because the mistake is a habit rather than an accident:
`grep -rln "from_le_bytes|\[off" src/`. `nvme_log.rs` was sixteen bytes out,
`hwmon/smart.rs` used the ioctl *input* layout to read the *output*, and
`edid.rs` never read the interlace flag. `disk/ata_smart.rs` is the one that
still cannot be checked here.

**A parse test whose fixture comes from the parser is a tautology.** It is what
let four NVMe fields ship wrong under a test named
`health_log_reads_every_field_at_its_specified_offset`. Write the fixture from
the spec document, or from bytes off real hardware; give every field a distinct
non-zero value so a shift cannot pass; and **confirm the test fails against the
old code before believing it**. Both parser fixes this session were checked that
way, and both did fail first.

**Eighteen defects came out of verifying a batch that arrived type-checked,
linted and looking finished.** Two were caught by tests that already existed.
The rest came from running the gate, reading snapshot output, CI, one new test,
and a grep over comments admitting the code could not determine something. That
ratio is the argument for every rule in this file.

**Running the gate was worth it, which is the point of the rule.** The batch was
type-checked and linted and looked finished; `cargo test --all-features` failed
on `non_nullable_entities_are_never_null` with five violations. Two causes, both
wrong declarations rather than broken readers:

- The CPUID triple was left non-nullable by the very fix that made it honest.
  Withholding family/model/stepping on Windows is correct, and the entity then
  has to say it may be absent.
- `system.printer.{n}.{connection,status}` — the Windows spooler reports
  `PrinterStatus` 1 and 2, which *are* "Other" and "Unknown", and the connection
  is classified from a port string that does not cover every local printer.

Generalising, because it will recur: **a fix that makes a reader stop lying
usually makes some entity nullable, and the declaration does not follow
automatically.** Both defects here were introduced by honesty fixes.

CI then found two more of exactly that shape — `board.input.{n}.interface` and
`memory.bandwidth.generation`, `Unknown` on both runners, fine here. Seven in
total from this batch.

**Those two are worth more than their size, because they name a blind spot this
file did not have.** The recorded lesson is that a local Windows run says
nothing about Linux or macOS. These were not that: **macOS passed**, and the two
failures were on *different operating systems*. What the runners share is that
they are **virtual machines** — a virtualised input device reports an interface
nothing classifies, and a VM's SMBIOS names no memory generation — and this
desktop is bare metal.

So the axis is hardware, not platform, and **no cross-target check can find it**:
the code compiles identically either way and only the readings differ. The two
`cargo check --target` commands above, which are the recommended cheap
substitute for other platforms, would have passed. Anything resolving off
SMBIOS, a device enumeration or a bus classification can differ this way, and
CI is the only instrument here that sees it.

That loose thread was pulled, and it ran the other way. `memory.bandwidth.
{achievable,stream_triad}` resolving while `generation` was unavailable did not
mean the declaration overstated the dependency — the derivation genuinely needs
the generation and **proceeded without it**, substituting 3200 MT/s, a 64-bit bus
and a 0.75 efficiency factor. Every VM, both CI runners included, was being
handed built-in defaults dressed as `Specification` and `Derived` readings. Fixed
in `2687dbb`.

The lesson is about reading evidence, not about memory: a derived value resolving
while its declared input is absent is *more* suspicious than it looks, not less.
The comfortable reading was that the schema was too strict. The true one was that
the code was inventing numbers.

### What is verified, and what is not

Verified on stable 1.98.0: `cargo fmt --all -- --check`,
`cargo clippy --all-features --all-targets -- -D warnings`, and
`cargo test --all-features --lib --tests --no-fail-fast` — green on every
target, **849 in the lib**, `ontology_conformance` 22, `plausibility` 12,
`agentic_contract` 16, `honesty` 7.

Also run and green: `cargo test --all-features --doc` (**73 passed**),
`cargo run --example probe_readers --all-features`, and
`simon snapshot --format text`.

**The full gate, in the order it should be run** — the three per-run holes found
this session are the last three lines, and each was found by CI after a local
run came back clean:

```bash
cargo fmt --all -- --check
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features --lib --tests --no-fail-fast -- --test-threads=4
cargo test --all-features --doc                     # excluded by --lib --tests
cargo check --quiet --no-default-features --all-targets                # (none)
cargo check --quiet --no-default-features --all-targets --features cpu
cargo check --quiet --no-default-features --all-targets --features cli
cargo check  --target x86_64-unknown-linux-gnu  ... # per-OS compile
cargo clippy --target x86_64-unknown-linux-gnu ... -- -D warnings  # per-OS dead code
```

**Run the tests last, after every edit including this file.** `HANDOFF.md` is an
input to `documentation_links` and `source_hygiene`; editing it after the gate
invalidates the gate, which broke CI once here.

That last one is the check the handoff keeps insisting on, and it paid again:
**the camera fix is now confirmed against hardware rather than against a
narrowed query.** `board.camera.{0,1}` are a Lenovo 510 IR and a Lenovo 510 RGB
— one dual-sensor Windows Hello webcam, correctly two rows — and the Brother
scanner is gone. Before the fix it was a scanner wearing a camera's name.

**CI is green on all three platforms**, on `master` at `1947426` — Format,
Clippy, Check, Feature combinations, and Test on windows, macos and ubuntu.

The MSRV is built rather than inferred: `cargo +1.89 check --all-features
--all-targets` passes, which also settles that `slice::as_chunks` in `da769bf`
exists at the declared floor.

### Run it and read the output — the only method that found any of this

Grepping for `unwrap_or` finds candidate sites. It does not tell you which of
the 884 matter, and it never once pointed at the worst defects. Every serious
finding in the `hardware_ai` and `ai_api` work came from **running the thing and
reading what it said**, then checking each claim against the machine:

- The inference report said "No SSD detected — HDD will severely bottleneck
  modern workloads" at 0.85 confidence on a host with three NVMe drives.
  `Get-PhysicalDisk` settled it in one command.
- It classified a desktop tower as a GamingLaptop.
  `(Get-CimInstance Win32_SystemEnclosure).ChassisTypes` returned 3.
- It dated a Ryzen 9 9900X to 2018.
- It reported 4GB of VRAM on a 24GB card, and recommended against ML training
  on that basis.

None of those are visible in a diff, and none would be caught by a test that
asserts a report *is produced*. They are only visible when a human-or-model
reads the sentences and asks whether each is true of this machine.

**Check ground truth in a second tool before believing the fix.** Every one of
the above was confirmed with a PowerShell query independent of the code path
being fixed, and the derived `boot_time` was checked against the kernel-start
event in the System log — two seconds apart, from sources that share nothing.
The one time this session that a plausible reading went unchecked, it was the
0.61-second boot duration, which was two timestamps of the same instant
subtracted from each other.

**A fallback is how a module fails completely and still looks like it works.**
`Get-PhysicalDisk | ConvertTo-Json` serialises MediaType and BusType as the
strings `"SSD"` and `"NVMe"`; the code read them with `as_u64()` and got `None`
for every disk on every Windows machine since the module was written.
`unwrap_or(0)` turned that total failure into a confident, plausible, wrong
answer. Nothing was ever logged, nothing ever errored, and the module kept
producing a full report.

**Watch for a guess placed in front of a reading.** Twice in one module: Windows
inferred the chassis from battery presence while Linux read the real DMI code,
and then the classifier scored a battery 0.4 against a read chassis at 0.3. The
same shape appeared in the ontology the same day, where `system.boot.secure_boot`
was bound to a reader needing Administrator while the value sat in the registry,
unelevated. When two sources disagree, check which one is *read* and which is
*inferred* before tuning weights.

### And the gate must include doc-tests

`1c88d93` failed CI on all three runners, on three doc examples in `lib.rs` and
`process_monitor.rs` that did arithmetic on a field which had become `Option`.

`cargo test --lib --tests` — the command this file recommends, for the good
reason that it is faster — **excludes doc-tests**. CI runs plain
`cargo test --all-features`, which includes them. Any change to a public
field's type can break a doc example, and nothing else in the local gate
compiles them.

```bash
cargo test --quiet --all-features --doc     # 74 of them, ~9 seconds
```

Nine seconds, and it is the third distinct hole in the local gate found this
session, after the feature-gated and target-gated ones. All three share a shape:
**CI runs a command the local gate does not, and the difference is invisible
until it fails.**

### Nor a target-gated one, for a different reason

`c14e8c6` failed CI's Clippy job on Linux and macOS: `classify_pnp_entry` and
its helpers are called only from `refresh_windows`, so they are dead code
everywhere else.

**The two `cargo check --target` commands below cannot catch this, and it is
worth knowing why.** `check` reports dead code as a warning; it does not deny
it. CI runs `clippy -- -D warnings` separately on each OS. So the same code
passes every local command and fails on two of three runners.

The cheap catch is to run clippy for the target rather than check:

```bash
cargo clippy --quiet --target x86_64-unknown-linux-gnu --all-targets   --no-default-features --features cpu,npu,io,network,amd,nvidia,intel -- -D warnings
```

It cannot use `--all-features` — `ring` needs a C cross-compiler — so it reports
two pre-existing dead items that the narrower feature set excludes
(`agent::backend::body_has_array`, `tuning::CLASSIFY_SYSTEM_PROMPT`). Those are
not CI failures; only *new* names in that output are.

**Anything added beside `#[cfg(target_os = ...)]` code needs this check**, which
is the same rule as the feature one above, one axis over: `src/bluetooth/`,
`src/display/`, `src/usb/` and `src/platform/` all hold per-OS readers next to
shared helpers.

### The local gate cannot see a feature-gated break

`cargo clippy --all-features`, `cargo test --all-features` and **both**
cross-target checks share one blind spot: every one of them enables `nvidia`.
The cross-checks name it in their feature lists explicitly. So a helper written
at module scope without `#[cfg(feature = "nvidia")]` passes the entire local
gate and fails CI's *Feature combinations* job, which builds each feature alone.
That happened at `7c79f8b`, on a `pstate_name` helper — the fix for one defect
breaking a build the gate does not cover.

Before pushing anything that touches a vendor-gated module, check the feature
alone:

```bash
cargo check --quiet --no-default-features --all-targets --features cpu
```

The full loop CI runs is twenty features and takes far longer than a local
session usually allows; the cheap approximation is `(none)`, `cpu` and `cli`,
plus whichever feature the file you touched is *not* gated by. `src/profile/`,
`src/hwmon/` and `src/backend.rs` all hold vendor-gated code beside ungated
code, which is where this is easy to get wrong.

### The MSRV is 1.89, and one value now covers every feature combination

It was 1.88, which held for a default build and not with `vault`:
`ironvault` requires 1.89 and **all fourteen of its published versions do**, so
there was nothing to pin back to. The choice was between claiming 1.88 with a
footnote or claiming what is true everywhere.

Raised to 1.89. A field meaning "1.88, except…" is the same shape of defect as an
entity declaring a provenance stronger than a row can carry — the reader has to
know the exception to avoid being misled. `cargo +1.89 check --all-features
--all-targets` now verifies the whole thing in one command, instead of two with
one expected to fail.

Note what found it: the documented MSRV check builds default features, CI builds
`--all-features` on stable, and the failing combination was the one neither ran.
That is the *Feature combinations* lesson one level down, in the dependency graph.

### A reader wired into one consumer leaves the others fabricating

`platform::macos::read_cpu_stats` and `read_memory_stats` were added in 5.2.0 and
wired into the ontology resolver. **`MonitoringBackend` — what the `simon` CLI
runs — kept its own hand-rolled macOS arm for two releases**, and almost every
field it produced was invented: the system-wide figure repeated across every
core, a fixed 60/40 user/system split, `governor: "performance"` on a platform
with no governors, `min = max/2`, and `"Apple CPU"` when the brand string was
missing. A failed load-average parse fell to 0.0 and published 100% idle.

So one Mac had two CPU paths that disagreed — one measured, one invented — and
`update_memory` had no macOS arm at all, so `simon` reported no memory there.
Fixed in `24a7314`.

**Three things worth carrying:**

- Adding a reader is not the same as adopting it. Grep for the other consumers
  of whatever the new reader replaces, because the old fabrication keeps running
  where it was not swapped in, and no test compares the two paths. **There were
  three consumers of the macOS memory path, not two** — the ontology adopted the
  real reader in 5.2.0, `MonitoringBackend` kept fabricating (`24a7314`), and
  `ai_api::tools` kept a third hand-rolled copy (`1cf1980`) with a hardcoded page
  size. Count the consumers before assuming a migration finished. **Then there
  was a fourth**, and it was CPU rather than memory: `ai_api::tools`'
  `get_cpu_status` shelled out to `sysctl vm.loadavg` and published
  `load / ncpu * 100` as `usage_percent`, next to `read_cpu_stats` it never
  called. Load average is not utilization — it counts runnable *and*
  uninterruptible tasks, so a machine blocked on I/O reads high while its cores
  idle. Grepping for the reader's name finds the consumers that adopted it; the
  ones still fabricating do not mention it, so grep for what they call instead.
- **A fabricated number is more dangerous than an absent one.** The nine silent
  readers reported nothing; this reported a believable 43% on every core, which
  a user cannot tell from a measurement.
- It was found by grepping comments — `requires admin`, `assume`, `approximation`,
  `hardcoded`, `placeholder` — next to returned values:

  ```bash
  grep -rn --include=*.rs -iE "//.*(requires admin|assume|approximation|hardcoded|placeholder|can'?t (detect|read))" src/
  ```

  **A comment admitting the code cannot determine something, sitting beside a
  returned value, is a reliable marker for a fabricated reading.** Run it before
  trusting any reader.

  **A second grep finds a different and larger population** — the defects nobody
  bothered to comment on:

  ```bash
  grep -rn --include=*.rs -E "unwrap_or\((0|0\.0|false|true|1)\)" src/
  ```

  This one needs triage rather than blanket treatment: `unwrap_or(true)` as "no
  filter given, include everything" is correct and common. **The signal is a
  fallback in a *reader* path**, where the value flows outward as a reading. It
  found the worst defect of the sweep — a hardcoded page size on the agent-facing
  memory tool that would report memory four times too large on an Intel Mac.

### What that sweep found, and the one class it turned up

A second pass over the four modules the grep flagged found one live defect and
two latent ones. The distinction matters and is easy to overstate:

- **Live: `ai_workload` reported any GPU-holding Python process as PyTorch.** The
  guess reached users through `signals.ai_frameworks`, which `tuning::classify`
  publishes beside the evidence string *"an AI framework was identified in a
  running process"* at 0.8–0.9 confidence. Fixed in `7b606fe`.

  **This is a different failure mode from the rest.** Every other defect this
  round invented a *number*; this invented an *identity*, and a second module
  then attested to it. A wrong number invites a sanity check; a plausible
  framework name does not — and `simon tune` is where acting on it has
  consequences.

- **Latent, fixed:** `dma_engine` claimed memcpy support whenever the `cap` file
  was unreadable. Nothing calls it.
- **Latent, now fixed:** `cpufreq::is_turbo` returned `bool` where it meant
  "cannot tell". Closed above; `Option<bool>`, and the ">95% of max" branch is
  gone rather than kept. **It was the pattern in *Queued for the next major
  version* recurring exactly as that
  section predicted**, and it is the first entry for 7.0.0.

`src/audio/mod.rs` was checked and is clean — the "placeholder" comment is in a
test, not a reader.

### Two local failures that were the machine, not the code

Both look exactly like defects and neither is. Recognising them saves an hour:

- **`LNK1104: cannot open file …-<hash>.exe`** means a stale test binary from a
  killed run is still holding it. `ps -W | grep <testname>` finds it; it will
  reproduce on every build until killed. Nothing to do with the quarantine below,
  though the symptom is similar.
- **`memory allocation of N bytes failed`** during `cargo test --lib` is an OOM
  abort, not an assertion. `--no-fail-fast` runs every target's binary and each
  spawns a thread per core. `-- --test-threads=4` completes cleanly.

Same lesson as the red pipeline, one level down: **a broken instrument and a
real failure produce similar-looking output**, and the difference is in the
message rather than the exit code.

### The environment ate most of this session

Windows Defender was quarantining Rust binaries and build artifacts live.
Symptoms, so the next person recognises it in one minute rather than an hour:
`rustc.exe` vanishing from a toolchain *while running*; builds dying at
`Compiling silicon-monitor` with exit 127; `EPERM` on deleting anything under
`~/.rustup`; every toolchain's `.rlib` files gone (21 left of 81,034 files) and
then partially reappearing with identical hashes. `rustup` cannot self-repair
through it, because it cannot delete what it cannot delete.

```powershell
Add-MpPreference -ExclusionPath "$HOME\.rustup","$HOME\.cargo","<repo>\target"
Get-MpThreatDetection | Select-Object -Last 20
```

**Do not attempt a `rustup toolchain uninstall` to fix it.** It was tried here
and made things worse: the uninstall half-completed against undeletable files,
leaving stable with no manifest and no `bin/`. It recovered on its own; the
lesson is that the failure is underneath rustup and rustup has no answer to it.

The version is 6.0.0 and is **not tagged**; `v5.2.0` is many commits behind.
Publishing to crates.io was explicitly declined by the maintainer for 6.0.0 and
5.2.0 — tag only.

## Released

| Version | State |
|---|---|
| 3.0.0 | Published, tagged `v3.0.0`. SMART and NVMe support. |
| 3.0.1 | Feature-combination fix — see CHANGELOG. |
| 3.1.0 | Windows NVMe passthrough (unelevated) and macOS CPU/memory. Published, tagged. |
| 3.2.0 | macOS per-core CPU, NVMe current power state, `--all-targets` in the feature job. Published, tagged. |
| 3.3.0 | Windows ATA SMART (unelevated), shared SMART collector, ontology to 94 entities. Published, tagged `v3.3.0`. The ATA parse is unverified against real SATA hardware — see open work 1. |
| 3.4.0 | DIMM slot topology and USB in the ontology (113 entities). Published, tagged `v3.4.0`. |
| 3.5.0 | Ontology-driven conformance tests, Windows PCI reader fixed, PCI domain (123 entities). Published, tagged `v3.5.0`. |
| 3.6.0 | PCIe link state on Windows via cfgmgr32; the PCI domain fully resolves. Published, tagged `v3.6.0`. |
| 3.7.0 | Virtualization, NUMA and ECC in the ontology (134 entities); two misleading virtualization readings withdrawn. Published, tagged `v3.7.0`. |
| 3.8.0 | `simon tune`: use-case detection and profile recommendations, with an automatic server. Recommend-only by default. Published, tagged `v3.8.0`. |
| 3.9.0 | Headless GUI reads four previously unreadable tabs; racy coverage test fixed. Published, tagged `v3.9.0`. |
| 3.10.0 | Hyper-V root partition distinguished from a guest via CPUID leaf 0x40000003; bare metal no longer reported as a VM. Published, tagged `v3.10.0`. |
| 4.0.0 | GUI rebuilt on Dewey; the ~10k-line egui implementation and eframe deleted. MSRV 1.85. Published, tagged `v4.0.0`. **Withdrawn in 5.0.0.** |
| 4.0.1 | GUI moved to Dewey's `agpu` backend over a wgpu Vulkan spec violation. Published, tagged `v4.0.1`. Withdrawn. |
| 4.0.2 | Dewey GUI made legible and its tabs clickable. Published, tagged `v4.0.2`. Withdrawn. |
| 4.0.3 | Dewey GUI restyled onto the recovered `CyberColors` palette. Published, tagged `v4.0.3`. Withdrawn. |
| 4.0.4 | Dewey Overview rebuilt from the egui widget vocabulary. Published, tagged `v4.0.4`. Withdrawn. |
| 5.0.0 | **The Dewey port is withdrawn and the egui GUI restored.** Claimed MSRV back to 1.70 — which was not true, see 5.2.0. Published, tagged `v5.0.0`. See open work 9. |
| 5.1.0 | Applied settings are reversible: `read_current`, `ApplyOutcome.previous`, `revert_setting`, `revert_cycle`. Published, tagged `v5.1.0`. See open work 13. |
| 5.2.0 | `simon ai models` reads an IronVault vault, behind an optional `vault` feature. **MSRV corrected to 1.88**, having been wrong since 5.0.0. The tuning loop is closed (open work 13). **CI made green after twelve consecutive failures**; macOS CPU/memory wired into the resolver. Tagged `v5.2.0`, **not published**. |
| 6.0.0 | **Zero-constructors renamed to `empty()`** and the `Option` refactor on `SwapInfo`/`RamInfo` done — all three "queued for the next major version" items closed. Ontology grew a capability and vocabulary layer with tests derived from the declarations; JSON-LD output with QUDT units; `simon status` (coloured, per-OS ASCII art); network and file intrusion detection; the tuning ledger. **Not tagged, not published.** |
| 2.1.5 | Committed, never published. Documentation only; superseded by 3.0.0. |

## Verification that is worth repeating

```bash
cargo fmt --all -- --check
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo test                       # default features: the `vault` tests must drop out
cargo +1.89 check --all-features --all-targets  # the declared MSRV, actually built
cargo check --target x86_64-unknown-linux-gnu --all-targets --no-default-features --features cpu,npu,io,network,amd,nvidia,intel,nvml
cargo check --target aarch64-apple-darwin --all-targets --no-default-features --features cpu,npu,io,network,apple,nvidia,num_cpus
```

**`nvml` is in that list because leaving it out cost four days of red CI.**
`nvml = ["nvidia"]` gates `src/platform/linux/gpu.rs`'s NVML reader, and enabling
`nvidia` alone does *not* enable it — so the feature subset above compiled
happily while `cargo check --all-features` on Linux did not. See
*The line that made every Linux job red* below. When adding a feature to
`Cargo.toml`, add it here too, or it joins the set of code no local check reads.

**The cross-checks cannot use `--all-features`, and the reason is not obvious.**
`gui` pulls `egui_extras → ehttp → ureq → rustls → ring`, and `ring`'s build
script needs a C cross-compiler for the target. Without one the check dies in a
build script having compiled none of this crate's own code, which reads at a
glance like a broken toolchain rather than a missing one. Keep the feature lists
above; `cargo tree -i ring --target <t> -e normal,dev` is how that was traced.

The corollary is that a feature combination the cross-check *can* build is one
CI never tries — CI checks `--all-features` and each feature *alone*, never the
subset above. `examples/cpu_monitor.rs` failed to compile on macOS without the
`apple` feature for exactly that reason and nothing noticed until a cross-check
ran with `--all-targets`.

**Build the MSRV, do not declare it.** `rust-version` said 1.70 from 3.x through
5.1.0 and had been false since 5.0.0: restoring the egui GUI brought in
`image 0.25.10` and `home 0.5.12`, which declare 1.88 themselves, and `image`
reaches `pxfm`, an edition-2024 crate that pre-1.85 cargo cannot even parse. Two
releases shipped a floor nothing had ever compiled against, because nothing did.
`cargo +<msrv> check` is one command and it is the only thing that makes the
field mean anything.

The corollary bit in the other direction too: raising the declared floor to 1.88
un-gated ten clippy lints that `incompatible_msrv` had been silently suppressing
at 1.70. A wrong `rust-version` does not merely mislead readers — it changes
which lints run.

**Read CI before believing a green local run.** Every job had failed on every
run for over a week — twelve consecutive, spanning the 5.0.0 and 5.1.0 releases —
while local suites on Windows were green throughout. Seven defects were hiding in
there, and each was invisible until the one before it was fixed, because a
failing job cancels the rest of the matrix.

Three of the seven could not be reproduced on this machine on principle: two
needed a box with no GPU, one needed a Mac. Two more lived in
`cfg(target_os = "linux")` code that a Windows compiler never reads. **A local
`cargo test` says nothing whatsoever about the other two platforms**, and saying
"the full suite passes" on the strength of one is how those shipped.

```bash
gh run list --limit 5    # before claiming anything is green
```

The failure mode to recognise: a pipeline that is always red and a test suite
that checks nothing are the same instrument. Both return a constant, and a
constant carries no information. This project has now been bitten three times —
839 passing tests over a GUI that reported no GPUs on a three-GPU machine, an
intermittent failure at three runs in five that read as noise, and this.

The two cross-target checks need only `rustup target add …` — no C toolchain, no
VM. They are the fastest way to catch platform breakage from a Windows box, and
they are how the 114 compile errors behind the 3.0.0 manifest bug were found.

**`--all-targets` is not optional there.** Without it `cargo check` builds the
library and skips the test targets, and `tests/macos_readers.rs` is
`cfg(target_os = "macos")` — so a signature change that broke it was invisible to
both the local suite and the cross-check, and reached CI. The `Option` fields on
`SwapInfo` did exactly that.

**A test can be wrong about units, and it fails on the machine that is right.**
The macOS job went red at `2b8bf16` on `memory_totals_are_internally_consistent`,
which asserted `ram.total >= 1 GiB` with the constant written in bytes — while
`read_memory_stats` divides `hw.memsize` by 1024 before storing it. The runner's
correct 7 GiB reading, 7340032 KB, read as 7 MB and failed the floor. The reader
was right and the assertion was wrong, and the failure message printed the word
"bytes" beside a KB figure, which is what made it take longer than it should
have.

Worth generalising: this crate stores memory in KB in `RamInfo` and in bytes in
several readers, and the compiler cannot tell them apart because both are `u64`.
Before believing a plausibility bound, check which unit the field is actually in.

CI additionally checks every feature in isolation (job **Feature combinations**).
That job exists because `--all-features` cannot catch a feature that only builds
because another supplies what it is missing — which is exactly how the `cli`
feature stayed broken through eight published versions.

## Open work

1. ~~**Dashboard metrics are unpublished.**~~ Done. Both renderers publish all
   24 names the bundled dashboards query, both pinned gap lists in
   `tests/prometheus_exposition.rs` are empty, and either list gaining an entry
   fails the build. The guard was split per publisher in `495e2ba`, the library
   exporter's four closed in `3ccaf43`, and the served endpoint's three in
   `400608c` and `cd07a27`.

   **One thing here is still unverified, and it is not the plumbing.** This
   machine reads zero CPU temperature sensors through all four Windows paths
   `hwmon` tries, so `simon_cpu_temperature_celsius` has been checked only in
   its absent form — a host with no sensor publishes no series rather than a
   `0`. The populated path is covered by a unit test against a synthetic sensor
   and has never met hardware that reports one. **If you are on a machine with a
   readable CPU sensor, scrape the endpoint and check the value against
   something else.** That is a minute's work and nobody has been able to do it.
2. ~~**USB negotiated speed is unimplemented on Windows.**~~ Done in
   `2c80d66`: 33 of 41 devices report a measured speed, and the eight that do
   not are six root hubs and two USB4 nodes, which sit on no upstream hub port
   and so have no link to describe. **The item below was wrong about the
   blocker** — kept here because the error is the useful part.

   ~~**Original text.**~~
   `usb.{addr}.speed` is absent on every device. It is not a PnP property; it
   comes from `IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX` against the parent
   hub, addressed by the port in `LocationInformation` — which only 14 of this
   machine's 39 devices carry, so the other 25 need the parent traversal solved
   first. The entity is worth filling: it documents that "a super-speed device
   on a high-speed port reports high, which is how a wrong cable shows", and
   that diagnosis is unavailable today. Whoever does it should find a ground
   truth to check against before trusting the result; Windows reports this
   nowhere else.

3. **The absence-reason audit is past half.** Every `unavailable` reading carries
   a prose reason, and those reasons are *claims about the platform — and
   sometimes about simon* — that can be checked. On this machine there are now
   **438 absences across 47 distinct reason strings**, out of 1804 readings, down
   from 465 out of 1797; four passes closed twenty-eight readings and opened six
   more that a contradiction had been hiding.

   **Forty of the 47 have been checked, and seven have not.** Twenty-four checks
   came back false, and each was a reading something already had — the first nine
   here, the rest in the sections above:

   | Reason | Rows | What was actually true |
   | --- | --- | --- |
   | "Windows reports link training nowhere simon can reach unelevated" | 164 | It reaches it for 23 devices *in the same snapshot*. Windows populates the properties for endpoints only |
   | "SMBIOS reported no operating voltage" | 2 | `Win32_PhysicalMemory.ConfiguredVoltage` = 1100 mV. The query did not select the column |
   | "the CPUID family/model/stepping triple was not read on this platform" | 3 | `Win32_Processor.Description` is "AMD64 Family 26 Model 68 Stepping 0" |
   | "the platform reported no line size for this cache" (+ sharing) | 6 | `GetLogicalProcessorInformationEx`, which this module's own docs claimed to use |
   | "simon has no mixer binding on this platform" (+ default endpoint) | 12 | `IAudioEndpointVolume`. True about the reader, not about Windows |
   | "this device declares no class of its own … no interface class or hub identifier was recorded" | 6 | `USB\ROOT_HUB30` is in every root hub's `HardwareID`. The query selected `CompatibleID`, which Windows leaves empty on a root hub |
   | "reader returned an empty string" — `board.input.{n}.vendor` and `.product` | 11 | The vendor and product ids are in the instance path, the entity asks for "name **or numeric id**", and the Linux reader has always published them |
   | "reader returned an empty string" — `board.camera.{n}.driver` | 2 | `Win32_PnPEntity.Service` is `usbvideo`, the direct analogue of the `device/driver` symlink the Linux reader follows. Not selected |
   | "no memory controller exposed ECC reporting; the platform interface exists but enumerated nothing" | 3 | No interface exists and none was consulted: `EdacMonitor::scan()` returned an empty overview on every platform but Linux |

   **Nine were true** and are recorded so nobody re-checks them: no NUMA affinity
   on any of 64 PCI devices, no negotiated link rate on WAN miniports and
   disconnected adapters, no minimum core frequency anywhere in `Win32_Processor`,
   no port count in `Win32_SCSIController.MaxNumberControlled`, no NVMe data on
   `disk.0` — a USB mass-storage gadget, while the three real NVMe drives report
   everything — no driver on exactly 15 of 64 PCI devices, blank `Description` on
   all four printers, class `00` on all nine remaining classless USB nodes, and a
   hypervisor masking the CPU virtualization bits (`HypervisorPresent` is true,
   VBS is running, and `Win32_Processor.VMMonitorModeExtensions` reads false while
   `VirtualizationFirmwareEnabled` reads true).

   **A tenth reason was true but the tempting fix is wrong, which is worth more
   than the check.** "This entry carries no USB vendor id; root hubs and virtual
   devices have none" (8 rows) survives contact with the hardware ids that just
   fixed the class beside it: `USB\ROOT_HUB30&VID1022&PID43FD` does carry a
   vendor and a product. They are the *PCI* ids of the AMD host controller,
   reused by Windows to name a synthetic device — publishing them in a USB
   vendor field would be a category error dressed as a fix, and the six root
   hubs would have reported a USB vendor that never appeared on a USB bus.

   **The method is the point, and it is cheap.** Group the absences by reason,
   take the largest, and ask Windows the question the reason answers — usually
   one `Get-CimInstance` or `Get-PnpDeviceProperty`. Reading the code instead
   finds nothing: in every failure so far the code was internally consistent,
   and a comment or a doc line stated the false belief outright. The EDAC
   module's header said "**Windows**: `wmic memorychip` for ECC support
   detection" above a function that has never run anything on Windows, and the
   resolver's comment for the branch that would have told the truth began "on
   Windows and macOS there is no EDAC equivalent simon reads, so this is the
   common path" — of a branch that was unreachable.

   **Two shapes account for the four found this pass.** Two were *a column the
   query did not select* — `HardwareID` for the root hubs, `Service` for the
   cameras, joining `ConfiguredVoltage` from the DIMM voltage that started all
   this. Two were *a field hardcoded empty beneath a reason that blamed the
   device*: `product: String::new()` on every Windows input device, and an empty
   overview returned by an EDAC scan that consults nothing off Linux. Both
   shapes are invisible from inside the module and obvious from one line of
   PowerShell.

   **Seven reasons have not been checked, and every one of them looks true by
   construction.** The 40 rate rows and 8 throughput rows are one sample where two
   are needed; the process list, the RAPL absence and the boot duration each name
   a boundary rather than a device; the board sensors are the missing hwmon
   binding of item 1, with `MSAcpi_ThermalZoneTemperature` answering "Not
   supported" on this board and no OpenHardwareMonitor namespace installed. **The
   two ATA attribute groups are the only ones that could still be wrong, and this
   machine cannot settle them** — it has no SATA drive, which is item 5.

   That is the end of what this machine can decide. **The next real progress on
   this item needs different hardware**, not more checking here: a laptop, a SATA
   drive, an Intel or AMD GPU on Linux.

   And one that is checked, true, and worth restating because 15 rows is a
   tempting target: `gpu.codec.{n}.max_fps` is a **deliberate** refusal, not a gap.
   The entity declares a derivation and the crate declines to invent the
   arithmetic; NVENC's `NV_ENC_CAPS_MB_PER_SEC_MAX` is the real source for it, and
   nobody has bound it.

   **Five shapes have accounted for every false reason found:**

   - *A column the query did not select* — `ConfiguredVoltage`, `HardwareID`,
     `Service`, `ExtendedPrinterStatus`, `LocationInformation`.
   - *A field hardcoded empty or zero beneath a reason blaming the device* — the
     input `product`, the camera modes, the EDAC overview, the AMD GPU's clocks
     and power.
   - *The first row of a list the platform never promised was ordered* — the
     display's two WMI nodes, `Get-NetTCPSetting`'s `Automatic` template.
   - *A reader that exists on another of this crate's own surfaces* — the active
     power scheme, read by `simon profile explain` and reported unbound by the
     ontology; the RNG instructions, detected two lines above the probe that
     reported none.
   - *A type that cannot say which path produced the absence*, so one sentence
     covers every case and fits none — the SMART counters, before `SmartSource`.

   The first three are invisible from inside the module and visible in one line
   of PowerShell. The fourth is invisible from PowerShell and visible only by
   asking the binary the same question twice, through two surfaces.

4. **`hardware_ai` was audited on one machine, and only one.** Every conclusion
   corrected in `a584dd0` and `7607401` was verifiably wrong on this desktop, and
   each fix was checked against a second source. That is not the same as being
   right in general. Two things specifically want a second machine before they
   are trusted:

   - **The classifier weights were rebalanced against a single data point.** A
     read chassis now scores 0.6 where a battery scores 0.25, and the Desktop
     branch is no longer gated on `!has_battery`. The *direction* is defensible
     on principle — a reading should outrank a guess — but the numbers are still
     tuned to one host. `test_laptop_bottleneck` and
     `test_gaming_desktop_classification` pass, and both use synthetic features
     rather than hardware. **Run `HardwareInferenceEngine::new()?.full_analysis()`
     on a real laptop and read the report.** If it does not say Laptop, the
     weights are wrong in the other direction now.
   - **`adapter_vram_bytes` was tested against NVIDIA and AMD adapters in one
     box.** Intel Arc and older drivers may not publish
     `HardwareInformation.qwMemorySize`. Those adapters get no VRAM reading at
     all, which is the intended failure — but nobody has watched it happen.

   ~~Also still imprecise, in a heuristic table rather than a reader: the RTX 3090
   Ti dates to 2020 because it matches the RTX 30 series rule, and the Ti shipped
   in 2022. `infer_gpu_year` and the TDP tables were not audited.~~ **Audited in
   `f4ea43d`.** The current year was the literal `2025.0` in three places; the Ti
   refresh is dated from its own release now; and the GPU TDP is the cap the
   driver enforces where one is reported, with `ThermalEnvelope::gpu_tdp_measured`
   saying which. Two limits remain and are described in the section above: the
   envelope counted one GPU on a two-GPU machine — fixed in `448a289`, see *The
   second card* — and `infer_cpu_tdp` was a family-wide table, now preferring
   RAPL's package limit where a platform states one (`9df8e67`), which Windows
   does not. **What is left of this item is a Linux machine**: for the classifier
   weights, and to run the CPU power-limit path that has never executed.

   **`cargo run --example hardware_inference --features cli` prints the report**,
   which is what this item asks you to read on a laptop. That example did not
   exist until `f4ea43d`; the instruction above could not be followed without
   writing it.

5. **The Windows ATA SMART path has never met a SATA drive.** 3.3.0 reads the
   attribute table unelevated through `IOCTL_STORAGE_PREDICT_FAILURE`, and the
   parse in `src/disk/ata_smart.rs` is tested only against buffers this project
   built. This machine has three NVMe drives and a USB gadget; on all four the
   path returns `NotSupported`, which exercises the decline and nothing else.

   **If you have a SATA SSD or HDD, that is the thing to use it for.** One run of
   `cargo run --example disk_monitor` against `smartctl -A` settles it. Watch
   power-on hours in particular: a minority of drives report that attribute in
   minutes, and nothing in the structure says which.

   **This is no longer hypothetical: the NVMe parser had exactly this bug.**
   `nvme_log.rs` omitted Controller Busy Time (111:96), so every field below it
   read sixteen bytes early — `power_cycles` returned Power On Hours,
   `power_on_hours` returned Unsafe Shutdowns, and `media_errors`, the field a
   person uses to judge whether a drive is dying, returned the error-log entry
   count. It shipped because `health_log_reads_every_field_at_its_specified_offset`
   built its fixture from *the parser's own offsets*: the test asserted that the
   code agreed with itself, under a name claiming it checked the specification.
   **A parse test whose fixture comes from the parser is a tautology.** Write the
   fixture from the spec document, give every field a distinct non-zero value,
   and confirm the test fails against the old code before believing it.

   Note for anyone re-reading the earlier plan: `IOCTL_ATA_PASS_THROUGH` was the
   scoped approach and it is a dead end. Its `CTL_CODE` carries `FILE_READ_ACCESS
   | FILE_WRITE_ACCESS`, so the access check happens before the driver is reached
   and a read/write handle on `\\.\PhysicalDriveN` needs Administrator — measured,
   `ERROR_ACCESS_DENIED` on all four drives. Whether an ioctl needs elevation is a
   property of its `CTL_CODE`, and is worth reading off the definition before
   planning around it.

6. **macOS GPU, power and temperature are still unimplemented.** CPU (per-core,
   with nice time), memory, swap, uptime and board info work — `Simon::cpu()`,
   `memory()`, `uptime()`. `Simon::snapshot()` still fails, because it requires
   every reader. Power and temperature need `powermetrics`, which requires root,
   so they may not be reachable unelevated at all; that is worth establishing
   before writing anything.

   **If you have a Mac, that is the thing to use it for.** These readers were
   written by cross-compilation and are verified only by `tests/macos_readers.rs`
   on `macos-latest`, which checks that readings are *plausible* — not that they
   are *correct*. Comparing `Simon::cpu()` and `memory()` against Activity Monitor
   once would settle whether the `vm_stat` accounting matches what a user sees.

   5.2.0 closed part of this. `platform::macos::read_cpu_stats` and
   `read_memory_stats` now exist and are wired into the ontology resolver, which
   previously had no macOS arm at all and reported "the platform CPU reader
   returned an error" for readers it had never called. Eight entities were
   affected, four of them non-nullable. They resolve on `macos-latest` now — the
   first time this code has executed anywhere.

   Still true, and still the reason this item is open: *plausible* is not
   *correct*. Nothing has compared these numbers against Activity Monitor, and
   the `vm_stat` used/free split is a judgement about which pages count as in
   use, which a conformance test cannot check.

7. **The Linux SMART/NVMe paths have executed exactly once**, in CI on
   `33ee241` — 733 tests, 0 failures. No one has run them against real Linux
   hardware. The sysfs paths (`/sys/class/nvme/<ctrl>/{model,serial,firmware_rev,cntlid}`)
   are documented kernel ABI, but tests are not a substitute for a drive.

8. ~~**`smart_disk()` spawns a subprocess per call.**~~ Fixed in 3.3.0 by
   `SmartMonitor::cached_disks()`, which shares one sweep process-wide for 2 s.
   A sweep is 1.23 s on this machine, and a four-drive pass could take twelve of
   them. Two things narrowed the problem before it was fixed: NVMe and SATA drives
   are now answered by their passthrough and never reach the collector at all, so
   what remains to benefit is USB storage — and every Linux machine, where a
   sweep spawns `smartctl` once per drive and the old shape was quadratic.

9. **The ontology names 255 entities; the library has 93 subsystem modules.**
   Both re-counted on 2026-09-08 and both still exact — `simon describe --format
   json` for the first, the module file list for the second.
   (Counted, not estimated: `simon describe --format json | .entity_count`, and
   the module files under `src/`. The previous figures here, ~232 and ~88, had
   drifted.)
   The running list of which clusters exist, which readers answer on which
   machine, and what is left is under **plan item F** below — it is kept in one
   place rather than two, because the last time it lived in both they disagreed.

   The standing rule is here because it governs every addition: **none should be
   declared until someone can watch its resolver answer on hardware that has the
   thing** — or, failing that, watch it decline with a true reason. A cluster
   that resolves to `unavailable` carrying a real explanation satisfies this. One
   that resolves to silence does not.

   **`tests/ontology_conformance.rs` now checks the things that used to need an
   eye.** Five entities across 3.3.0 and 3.4.0 shipped an enum's `Unknown` through
   as a *measured* value, each caught in review; that class is now caught by
   construction, along with non-nullable nulls, unit/JSON-type mismatches and
   absences with no stated reason. Run it before adding a domain and after — it
   found three defects on its first run, one of them an entity declared since the
   ontology was written that no resolver had ever touched.

   An id that resolves to a confident guess is worse for an agent than one that
   does not exist, so add a domain only when its resolver can say why each absence
   is absent.

10. ~~**`VirtMonitor::is_virtual_machine()` returns true on a Hyper-V root
   partition.**~~ Fixed in 3.10.0 via Hyper-V CPUID leaf 0x40000003: the
   partition privilege mask holds `CreatePartitions` and `CpuManagement` only on
   a root partition. `hypervisor_indicates_vm()` is now the single path, so
   `detect_platform()` and the ontology entity were corrected at the same time,
   and the 3.7.0 ontology workaround is withdrawn.

   **The guest side is still unverified against a real Hyper-V VM.** The root
   side is measured on this desktop (`ebx=0x002bb9ff`). If you have a Hyper-V
   guest, one run of `simon get system.virtualization.platform` settles it —
   expect `virtual_machine`. CI's Windows runners are Azure Hyper-V guests and
   exercise the arm, but no assertion pins the value there because the same test
   must pass on this bare-metal desktop.

11. ~~**The Windows PCI reader blocks the PCI ontology domain.**~~ Fixed in 3.5.0.
   The reader reported a device-instance path as the address and never reported a
   bound driver, so 3.4.0 withheld the domain rather than ship unstable ids and a
   false "no driver is bound". Both now come from
   `HKLM\SYSTEM\CurrentControlSet\Enum`, which is readable unelevated:
   `LocationInformation`'s parenthesised tail carries the bus/device/function
   triple, and `Service` carries the driver. Addresses are conventional BDF
   (`0000:7a:00.0`) and the domain ships.

   PCIe link width and speed followed in 3.6.0, from the device node property
   store via `CM_Get_DevNode_PropertyW`. Two notes for anyone extending this:
   `Get-PnpDeviceProperty` reads the same values and costs 0.4 s per device (25 s
   here, against 0.62 s for the whole monitor via `cfgmgr32`), and the registry
   property store under `Enum\...\Properties` is ACL-blocked unelevated, so
   neither is a shortcut. The remaining `DEVPKEY_PciDevice_*` properties — payload
   sizes, AER capability, ARI and ATS support, SR-IOV — are readable by the same
   two calls with a different pid, if anyone wants them.

12. **`simon tune`'s policy table covers five settings, and its game detection is
   a name table.** Both are deliberate first cuts, and both are where the feature
   grows.

   The policy table in `tuning::policy_for` is keyed on setting id, not on a
   category, because two vendors' "performance mode" are not interchangeable —
   pretending otherwise is how a tuner writes one vendor's value into another's
   register. Adding a setting means adding an apply handler first: a policy for
   something unwritable produces advice nobody can take, which is why the planner
   records it under `skipped` rather than recommending it.

   Game detection matches launcher and engine process names, which will miss most
   games. It is used only as corroborating evidence and carries lower confidence
   than an identified AI framework for that reason. The right answer is to ask
   the graphics driver what is presenting full-screen; that is not implemented.

   **The rule to preserve:** a proposed value comes from what the driver declared
   — a `choices` entry or the reported `default` — never from this crate and never
   from a model. `tuning::tests::a_recommendation_never_proposes_a_value_the_driver_did_not_offer`
   is the test that keeps it true. A model may classify; it may not pick numbers.

13. **The Dewey port was tried across 4.0.0–4.0.4 and withdrawn in 5.0.0.**
   `src/gui/` is the egui application again — `app.rs`, `widgets.rs`, `theme.rs`,
   `profile_tab.rs`, `headless.rs`, `mod.rs`, restored from `927ffaa^`. The
   `deweygui` dependency, the `dewey-gui` feature and `simonlib::gui_dewey` are
   gone rather than left as dead aliases.

   **Read this before proposing the port again.** The argument for it was sound:
   the 3.9.0 spinner bug came from a headless path that diverged from the
   interactive one, and Dewey's `Command::Task` removes that class of bug. What
   the argument missed is what the egui GUI actually does. Side by side, the
   original shows nine metric cards including live GPU temperatures, six
   sparkline charts with axes and min/max, a threshold legend, an uptime and task
   status line, an AI ask bar and JSON/CSV export — and on the development
   machine it detects three NVIDIA GPUs where the port reported
   `Accelerators: unavailable — Failed to initialize COM`.

   So the port was not merely unfinished to look at. **It was reading less
   hardware, and the tests said nothing about either fact.** They asserted that
   every tab emitted named ontology nodes, which the port did faithfully while
   displaying a fraction of what it replaced. A contract that checks the shape of
   the output and never its substance will certify a regression as complete. If
   this is attempted again, the acceptance criterion is a screenshot of both
   GUIs side by side and a diff of what each reports on the same machine — not a
   node count.

   Four releases went into making the replacement presentable and it never got
   close. Budget accordingly.

14. ~~**`CpuStats::new()` and `MemoryStats::new()` are zero-constructors with
   constructor-shaped names.**~~ Fixed in 6.0.0. Both are `empty()` returning
   `Self`, and `tests/zero_constructors.rs` holds an empty list: no `new()` in
   this crate fabricates a reading.

   **Five defects came out of this one pattern, across three separate
   discoveries** — two GUI call sites, `SiliconMonitor::snapshot_cpu` and
   `snapshot_memory` returning zeros from the *public* API, `SystemHealth::check`
   computing CPU usage from 100% idle so no threshold could ever be crossed, and
   the Prometheus exporter publishing 0% CPU on every scrape. Three of those were
   found only because the rename forced every call site to be read.

   The rule that outlived the bug: **assume any `T::new()` in this crate may be a
   zero-constructor until checked**, and prefer `empty()` for a builder base. The
   name is the whole defect — every call site was written by someone who
   reasonably believed `new()` constructs a thing from the system.

15. **Verify with `--lib --tests` when the disk is tight.** `cargo test
   --all-features` links every example. That is affordable again now the duplicate
   egui is gone, but if it ever fails with `link.exe` 1318, the split is
   `cargo test --all-features --lib --tests` for execution plus
   `cargo clippy --all-features --all-targets` for type-checking the examples.
   Note `--lib --tests` skips doc-tests; run those before a release.

   **`cargo`'s output does not go to `./target` on this machine.**
   `~/.cargo/config.toml` sets a `target-dir` outside the repo, at
   `~/.cargo-target`. `./target` used to hold a stale tree from
   before that line was added, so running `./target/debug/simon.exe` ran
   *whatever was built before the redirect*, silently — it cost a wrong
   conclusion during the USB speed work, where every device read `unavailable`
   from a binary that predated the reader and the finished feature looked broken.
   **That stale tree is deleted** (7.4 GB, in the disk-full recovery below), so
   the trap is gone until something recreates it. Invoke `cargo run --bin simon`,
   or the path `cargo build` prints.

   **Three failure modes here are the machine, not the code, and all three lie
   about it.** `error[E0463]: can't find crate for simonlib`, `error[E0786]:
   found invalid metadata files`, and `crate X required to be available in rlib
   format` are all truncated build artifacts, and the recovery is
   `cargo clean -p silicon-monitor`. What truncates them is resource exhaustion:
   the disk reached 100% of 3.7 TB twice in one session, and `rustc` was killed
   by `memory allocation of 69206032 bytes failed` while another process held
   45 GB of the machine's 94 GB. `cargo test -j 2` finishes where the default
   parallelism dies. Check `df -h` and free memory *before* concluding a change
   broke the build -- this has been misread as a tool bug four times.

   **It happened a third time, and the symptoms were different enough to fool
   another session.** The disk hit **0 bytes free of 3.7 TB** while the shared
   target directory held **235 GB**. What that looked like, in order:

   - builds dying at exit 255 mid-compile, with no error text at all;
   - a background task killed with "the system is running low on memory", on a
     machine with 24 GB free — the page file cannot grow on a full disk;
   - and finally the real message, `link.exe` **1140**, **1318** and **1201**,
     which are all PDB-write failures and all mean "no space" without saying so.

   Recovery was `Remove-Item` over the shared target directory (repeat the pass:
   files held open by other builds fail the first time) plus this repo's stale
   `./target`. That returned **224 GB**. `git fsck` afterwards showed only
   dangling objects and no corruption, which is worth checking every time —
   `~/.cargo/config.toml` records a git ref corrupted mid-commit by an earlier
   occurrence.

   **It refilled in nine hours and took a file with it.** The six other projects
   rebuilt everything that had been deleted, at roughly 30 GB an hour, and the
   disk hit zero again the same evening — this time in the middle of a script
   rewriting `HANDOFF.md`. Python's `open(path, "w")` truncates before it writes,
   so the failure left **a zero-byte handoff**, and the only reason nothing was
   lost is that the file had been committed twenty minutes earlier:
   `git checkout -- HANDOFF.md` brought it back whole.

   **Write through a temporary file and rename.** `os.replace` is atomic on
   Windows and on POSIX, so a full disk costs you the write and not the file.
   Every script in this session that edited a tracked file used the truncating
   form, and on any other evening that would have been fine — which is the point:
   the practice has to be in place before the disk fills, because afterwards
   there is nothing to fix it with except the last commit.

   **The lock is shared, and so is the queue.** `target-dir` is machine-wide, so
   *every* Rust project on this box serialises on one build-directory lock. With
   six other projects building — `tabinator-core`, `forgectl`,
   `robotcontrolleros-drivers`, `buildingops-service`, `aircom-ffi`,
   `avatar-processor` — a `cargo test` here can sit in "Blocking waiting for file
   lock on build directory" for tens of minutes and then be killed by whatever
   timeout is watching it. **That is not a broken build and not a broken tool.**
   `Get-CimInstance Win32_Process -Filter "Name='cargo.exe'" | Select CommandLine`
   shows who holds it. Do not kill them: they belong to other people's work.


16. **Two Dewey bugs found during the port, recorded because they are real
   and unfixed — but no longer reachable from this crate.** Neither affects simon
   now that `deweygui` is gone. Both are for whoever works on Dewey itself, or
   for anyone who reconsiders open work 9.

   *wgpu Vulkan semaphore reuse.* Dewey's default eframe path reuses one binary
   semaphore across swapchain images, which the validation layer rejects as
   `VUID-vkQueueSubmit-pSignalSemaphores-00067` on every frame. Undefined
   behaviour by the spec rather than noise, though it renders. The fault is
   wgpu's, below both Dewey and eframe. Dewey's `agpu` backend does not hit it.

   *Intermittent blank render.* Resizing an `agpu` window — including a
   `SetWindowPos` to the size it already was — sometimes leaves it entirely blank
   with the process alive and nothing logged. It reproduces in Dewey's own
   `counter_agpu` example. Instrumenting `agpu_backend.rs` showed winit
   delivering a spurious `Resized(<the configured size>)` immediately after the
   true size, shrinking the surface while the window stays large. Three fixes
   failed and were reverted rather than left in place: `ControlFlow::WaitUntil`,
   requesting a redraw on resize, and reconciling against `window.inner_size()`
   each frame — the last because winit's own `inner_size()` reports the stale
   value too. That is the detail any further attempt should start from. Repro:
   three `eprintln!` calls in `resize`, the surface-acquire error arm and the
   frame-area computation in `render`, driven by `ShowWindow(hwnd, 3)`.

17. **Readers written this session that no hardware has confirmed.** Each works
   or declines correctly here; none has been watched producing a value on the
   hardware it is for. Listed because "it compiles and the absent case is right"
   is exactly how a wrong reading ships.

   - **`simon_cpu_temperature_celsius`.** This machine reads zero CPU sensors
     through all four Windows paths `hwmon` tries, so only the *absent* branch is
     exercised. The populated path has a unit test against a synthetic sensor and
     has never met a real one. On a machine with a readable sensor: scrape
     `/api/v1/metrics/prometheus` and compare against anything else.
   - **`GpuMemory::unreported()` on three of four backends.** Apple, Intel-on-
     Linux and the two WMI capacity fallbacks all return it. The NVIDIA and
     Windows-AMD paths are confirmed; the rest are simple and unwitnessed.
   - **The Intel Linux `utilization`, which is a proxy and says so.** It divides
     the current graphics clock by the maximum. A throttled but saturated GPU
     reads low; one parked at maximum clock reads busy. `i915` exposes real
     engine-busy counters through PMU perf events and that is what this should
     read — the comment at the site names them.
   - **The USB speed reader's inheritance rule.** 33 of 41 devices answered here,
     and the eight that did not are six root hubs and two USB4 nodes, none of
     which sits on an upstream hub port. A machine with a Bluetooth or I2C HID
     device would exercise `classify_enumerator` arms this one never reaches.

18. **Applied settings are reversible; the tuning loop is not yet closed.**
   `ApplyHandler::read_current()` reads a setting before it is written,
   `ApplyOutcome.previous` carries what was overwritten, `revert_setting()` puts
   it back through the same confirmed and audit-logged path, and
   `tuning::serve::revert_cycle()` undoes everything one cycle applied, in
   reverse.

   Before this, `simon tune --apply` could change a machine and had no way to
   change it back: the trait could only write, and the outcome recorded only what
   was requested. That is worth remembering as a shape of bug — the write path
   was complete and correct on its own terms, and useless for anything that
   needed to undo it.

   **The rule to preserve:** when no prior value was recorded, `revert_setting`
   refuses rather than writing a default. Putting a machine into a state it was
   never in is a worse failure than leaving it where the caller put it. A handler
   without `read_current` makes its setting explicitly one-way; the default
   returns `None` so existing handlers still compile, which means *adding* a
   handler without it silently gives up reversibility. Implement it whenever the
   source can be read back.

   Verified on Windows: `read_current` returns the same GUID that
   `simon profile explain active_scheme_guid` reports through unrelated code in
   `profile::cpu`. **The Linux sysfs readers are written by inspection and have
   never run** — there was no Linux machine in the session that added them, and
   CI compiles the path without exercising a sysfs write.

   **The loop was closed in 5.2.0**, in `tuning::verify`: measure, write, settle,
   measure, revert on a demonstrated regression. `serve::cycle_verified` runs a
   pass that way and each `AppliedOutcome` carries its `Verdict`.

   The mechanism was the easy half, as expected. The hard half went the way the
   warning above predicted, and the result is worth stating plainly: **the metric
   registry ships empty.** The obvious metric for a power-scheme change is
   achieved clock speed, and on Windows it does not exist —
   `CallNtPowerInformation(ProcessorInformation)` reports a nominal figure.
   Sixteen spinning threads took system idle from 79.7% to 11.4% and every core
   reported exactly 4400 MHz throughout. A verifier built on that number would
   have reported "no change" for every power scheme ever written and nobody would
   have questioned it.

   So `metric_for` returns `None` for every setting, `Unverifiable` is the normal
   verdict, and `no_setting_claims_a_metric_it_has_not_earned` exists to make
   adding one a deliberate act. **The bar for adding a metric is a demonstration,
   on real hardware, that the number moves when the setting changes — in the
   commit message.** A registry entry added because the mapping seemed sensible
   is precisely the invented criterion the module was built to refuse.

   **The NVML lead was followed, and the answer is that a live metric exists but
   nothing on Windows can be verified with it.** Six samples via `nvidia-smi`
   half a second apart: GPU 0 moved 540 → 495 → 495 → 555 → 585 → 645 MHz while
   its utilisation moved 26 → 38%. That is a genuinely live reading, in direct
   contrast to the Windows CPU clock, which does not move at all. So the metric
   source is sound.

   It has nothing to verify. **Every setting whose effect a GPU clock could
   measure is Linux-only:**

   | Setting | Platform | Metric |
   |---|---|---|
   | `active_scheme_guid` | Windows | none — the CPU clock reads nominal, see above |
   | `scaling_governor` | Linux | plausible: `scaling_cur_freq` is live on Linux, unlike the Windows equivalent. Untested. |
   | `persistence_mode` | Linux + NVIDIA | none plausible — it governs driver load latency, not throughput |
   | `perf_level` | Linux + **AMD** | GPU core clock |
   | `gt_max_freq_mhz` | Linux + Intel GPU | GPU clock |

   So the verification loop **cannot be exercised end to end on Windows at all**:
   the one applicable setting has no live metric, and every setting with a
   plausible metric is on the other platform. `perf_level` compounds it by being
   AMD, where the GPUs here are NVIDIA — so even the vendor does not line up.

   No metric was registered, and no metric source was written, on purpose.
   Writing an NVML clock reader to verify an AMD sysfs setting on a Linux machine
   that does not exist here would be plausible, untestable, and exactly the class
   of work this project has been bitten by twice — the Linux sysfs readers in
   `profile::apply` are already written by inspection and have never run.

   **What actually unblocks this is a Linux box**, ideally with an AMD GPU. On
   one, the demonstration is cheap: sample the GPU clock, write `perf_level=high`,
   sample again, and show the number move. That measurement belongs in the commit
   that adds the registry entry.

   **A timed benchmark was tried as a way round the missing Windows metric, and
   it is a dead end twice over.** The idea: if the reported clock is nominal,
   measure achieved throughput instead. A fixed single-threaded integer workload,
   nine samples, median of each:

   | Scheme | Median | MAD |
   |---|---|---|
   | Balanced | 236.8 ms | 2.2 |
   | High performance | 239.2 ms | 2.3 |

   A delta of 2.4 ms against a noise band of 4.5 ms, with the sign backwards —
   High performance nominally *slower*. By `verify`'s own threshold rule that is
   `Unchanged`. A lightly loaded desktop boosts a single thread to the same place
   under both schemes, so there is nothing for the scheme to change.

   The second reason is the one that closes the avenue regardless of what a
   sustained all-core run would have shown: **a metric that requires running a
   benchmark is unusable in an autonomous tuner, because it competes with the
   workload it is tuning for.** Burning every core for two seconds to find out
   whether the machine got faster is self-defeating on a machine that was
   already busy, and meaningless on one that was not.

   So the metric has to be *passive observation of the real workload's own rate*.
   AI workloads and their frameworks are already detected; tokens per second off
   a running inference server is the shape of thing that would work — it is the
   user's own work, measured without perturbing it. That is the direction worth
   taking, and it is a larger piece of work than a registry entry.

   (Note for whoever repeats the benchmark: a constant seed makes the workload
   pure and LLVM eliminates every call after the first. The first attempt printed
   266 ms followed by eight zeros. `black_box` the inputs, not just the result.)

## Queued for the next major version

Three fixes that are correct, identified, and breaking. They live in code
comments at their sites, which is where the person fixing them will look and not
where the person planning a release will. Collected here for that reason.

Each is the same underlying fault: **a type with no way to say "not read"**, so
absence is encoded as zero and becomes indistinguishable from a measurement of
zero. That is the one distinction this crate exists to preserve, and these three
places cannot preserve it without an API change.

1. ~~**Rename the zero-constructors.**~~ Done in 6.0.0. They are `empty()`
   returning `Self`, and `tests/zero_constructors.rs` now holds an empty list:
   no `new()` in this crate fabricates a reading.

   **The rename was not cosmetic, which is worth remembering before deferring
   the next one.** Going to do it turned up three live defects the misleading
   name had been hiding: `SiliconMonitor::snapshot_cpu` and `snapshot_memory`
   returned zeros from the public API, `SystemHealth::check` computed CPU usage
   from 100% idle so no threshold could ever be crossed, and the Prometheus
   exporter published 0% CPU on every scrape. Five defects total from this one
   pattern, over three separate discoveries.

2. ~~**`Option` fields on `SwapInfo`.**~~ Done in 6.0.0. `total`, `used` and
   `cached` are `Option<u64>`, with `total_or_zero()`, `used_or_zero()`,
   `cached_or_zero()`, `is_reported()` and `swap_usage_percent() -> Option<f32>`
   for callers that genuinely want the old behaviour.

   The 62 read sites were the reason this was deferred, and they were the cheap
   part. What the change actually bought: it **ended three workarounds and one
   unit bug**. The macOS reader no longer fails the entire memory read to avoid
   claiming a machine has no swap — it says `None` and keeps the RAM figures.
   Linux's missing-`SwapTotal:` error path and the resolver's two-way swap
   branch both collapsed into the type. Deferring on cost was the wrong call:
   the cost was in the mechanical part and the value was in the parts that had
   been bent around the missing case.

3. ~~**`Option` fields on `RamInfo`.**~~ Done in 6.0.0. `shared` is
   `Option<u64>`; `buffers` stays a plain `u64` and stays 0 on macOS, because
   that platform keeps no buffer pool distinct from the file cache and the zero
   is a fact about the platform rather than a missing reading. That asymmetry is
   deliberate — do not "finish the job" by making `buffers` an `Option` too.

All three are closed. The section is kept because the pattern will recur: **a
type with no way to say "not read"** encodes absence as zero, and zero is
indistinguishable from a measurement. When the next one appears, the lesson from
items 1 and 2 is that the rename or the `Option` is not the work — the defects
it uncovers are, and there are always more than expected. Item 1 turned up five.

**It has recurred, and this is the 7.0.0 list.**

4. ~~**`BatteryInfo::charge_percent` reports 0% when the charge was not read.**~~
   **Done in 6.0.0.** `Option<f32>`, `push_opt` in the resolver, and the TUI
   prints "charge unread". Fixing it turned up a provenance bug in an unrelated
   entry — see the entry near the top of this file.
5. ~~**`cpufreq::is_turbo` returns `bool` where it means "cannot tell".**~~
   **Done.** `Option<bool>`, and the ">95% of max" guess removed with it — that
   branch was the one this entry under-described, and the one that fires on most
   Intel parts. See the entry near the top of this file.

6. **Historical metrics for the agent surface.** `get_historical_data` and
   `compare_metrics` were advertised in the tool catalogue — full descriptions,
   worked examples, `minutes_ago` parameters — and neither was ever implemented.
   An agent that read the catalogue and called one got "Unknown tool", which it
   would read as its own mistake. They were removed rather than stubbed, because
   a stub is the same lie with more code. Building them for real needs three
   things: timestamps on `backend::HistoryBuffer` (it is a bare `VecDeque` today,
   so "five minutes ago" is not answerable from it), a handle to that buffer from
   `AiDataApi` (which holds only `historical_context: Option<String>`, a blob a
   caller sets), and a decision about what happens when the window does not reach
   back as far as the question — which is an absence with a reason, not an
   interpolation.

7. **`simon cli --format json` emits kilobytes under keys that do not say so.**
   `RamInfo` and `SwapInfo` are in KB — documented on every field — and the CLI
   serialises them verbatim, so `"total": 98191140` is the same machine whose
   `simon snapshot` reports `memory.total = 100547727360` and whose Prometheus
   endpoint reports bytes. The human renderer beside it converts correctly and
   prints `93.64 GB`.

   **It is not a wrong number; it is an uninterpretable one**, and it sits on the
   fault line that produced `367429a`, where the exporter published this exact
   field unscaled under a `_bytes` name and reported a 93.6 GiB machine as having
   98 MB of RAM.

   Two ways to close it, and they differ in what they break:

   - **Scale at the CLI boundary**, so `--format json` reports bytes like every
     other machine-readable surface. Changes values a consumer may be parsing
     today.
   - **Name the unit in the output** — `total_kb`, or a `units` field. Additive,
     but it spreads a per-struct convention into every serialisation of these
     types, including the agent surface.

   The first is the one that ends the inconsistency; the second only documents
   it. Either is a breaking change to a published format, which is why this is
   here and not in a commit.

**Both were found by the fallback grep, and it is nowhere near exhausted:** 780
`unwrap_or(0|false|…)` / `unwrap_or_default()` sites outside tests and the GUI.
Most are legitimate. Triage by consequence rather than by count — a fallback in a
reader path that reaches the ontology or the agent tools is worth reading; one in
a filter predicate is not. **Triaging by blast radius rather than in file order
is what found the agent-surface defects:** a `0` in a TUI gauge is cosmetic, a
`0` handed to an LLM becomes a premise it reasons from. The Linux `/proc` parse
sites in `system_stats.rs` are deliberately untouched — they cannot be run or
observed here, and reworking unrunnable readers is the debt this project already
carries.

**The agent tool surface had no test at all until `tests/ai_tool_surface.rs`.**
That is why three fabrications lived in `ai_api/tools.rs` undisturbed. The suite
asserts about shape rather than about particular hardware, so it says the same
thing on a desktop and on a CI runner with no GPU. It found the two phantom tools
above on its first run.

## The plan for what is left

Items A–D are blocked on hardware this project does not have, and are
*verification* tasks with concrete recipes. E and F are ordinary software work
that anyone can pick up on any machine.

(This section previously opened by claiming everything left was hardware-blocked.
That was true when it was written and stopped being true one release later, which
is worth noticing: the sentence survived a commit that invalidated it.)

**A. Verify the ATA path against a real SATA drive.** *Needs: any SATA SSD or
HDD, on Windows. Half an hour.* **This is now retrospective.** The
recommendation was to hold the release until this check had been made; the
decision was to publish anyway, so 3.3.0 is on crates.io with the ATA parse
unverified. That raises the stakes rather than removing them: a defect found here
is a yank or a 3.3.1, not an unreleased fix.

1. Attach the drive — internal, or a USB-SATA enclosure whose bridge tunnels
   SMART (JMicron and ASMedia generally do; the cheapest ones do not).
2. `cargo run --example disk_monitor`, and `smartctl -A /dev/sdX` for the same
   drive.
3. Compare, in this order of importance: reallocated (5), pending (197) and
   uncorrectable (198) sector counts, since those are what `health()` grades on
   and what no other Windows path can reach; then temperature; then power-on
   hours.
4. If nothing appears at all, the parse refused the structure. The likely cause
   is the checksum — `AtaSmartData::parse` declines a structure whose final byte
   does not make all 512 sum to zero. Dump `VendorSpecific` and check by hand
   before loosening it, and if it must be loosened, note that smartmontools
   warns-and-continues here and this deliberately does not.
5. Watch power-on hours specifically. A minority of drives report that attribute
   in minutes rather than hours and nothing in the structure says which; if the
   number is 60× what it should be, that is what happened, and it is a per-vendor
   quirk table, not a bug in the parse.

Done when a real drive's attributes match `smartctl -A`. If they do not, the fix
ships as 3.3.1; if the readings are actively wrong rather than merely absent,
yank 3.3.0, because a wrong SMART attribute is the class of error that gets acted
on.

**B. Establish what macOS power and temperature can reach unelevated.** *Needs: a
Mac. An hour, before any code.*

This is research, not implementation, and doing it first is the point.
`powermetrics` requires root, which would make the obvious implementation
unusable in the same way `IOCTL_ATA_PASS_THROUGH` would have been — and the ATA
work is the argument for checking first. Run, as a normal user:

- `powermetrics -n1 --samplers cpu_power` — expected to fail; confirm it does.
- `ioreg -rc AppleSmartBattery` and `ioreg -rc IOPMPowerSource` for power.
- `ioreg -rc AppleSMC` and the `SMC` keys for temperature.
- On Apple silicon, `IOHIDEventSystemClient` temperature sensors, which are
  readable unelevated where SMC keys are not.

Write down which of these answer without root *before* writing a reader. If none
do, the honest outcome is a documented `NotSupported`, not an implementation that
only works under `sudo`.

**C. Validate the macOS readers that already exist.** *Needs: a Mac. Twenty
minutes.* Compare `Simon::cpu()` and `Simon::memory()` against Activity Monitor.
`tests/macos_readers.rs` checks readings are *plausible*, not *correct*; the
specific thing in doubt is whether the `vm_stat` accounting matches what a user
sees under "Memory Used", which is a different figure from free-plus-inactive.

**D. Run the Linux SMART/NVMe paths on real hardware.** *Needs: a Linux box with
an NVMe drive. Twenty minutes.* `cargo run --example disk_monitor` against
`nvme list` and `smartctl -A`. The sysfs paths are documented kernel ABI and the
code has passed CI, but CI has no drive. The quadratic collector shape fixed in
3.3.0 was worst on Linux, so this is also the first real check of that.

**E.** ~~Read PCIe link state on Windows.~~ Done in 3.6.0; see open work 7 for the
two routes that do not work and why.

**F. Continue the ontology sweep.** *Needs: nothing. Ongoing.* NUMA,
virtualization and EDAC landed in 3.7.0. **RAPL landed in 6.0.0** —
`power.rapl.{n}.{name,energy,max_energy_range,power_limit,enabled}` plus a
`power.rapl.<none>` diagnostic.

Two things that sweep turned up, both worth repeating for the next cluster:

- The reader returned `Ok(vec![])` on Windows and macOS with a comment
  explaining why there was nothing. An empty list is what a Linux box with all
  zones disabled returns, so the reason lived in a comment where no caller could
  reach it. It is now an error carrying the reason, and the resolver turns that
  into `power.rapl.<none>` with the text.
- The `<none>` diagnostic is declared per *domain*, and RAPL is a cluster inside
  `power`. Reusing `power.<none>` would have claimed the whole domain enumerated
  nothing on a machine whose battery reads fine, so the sub-cluster gets its own
  declared diagnostic. Expect the same for the next cluster inside an existing
  domain.

**Sensors landed too**, as `board.sensor.{n}.*` with a `board.sensor.<none>`
diagnostic. It was left out of the first pass on the grounds that the reader
returns nothing on this machine — which was inconsistent, because RAPL returns
nothing here either and was declared anyway. A cluster that resolves to
`unavailable` carrying a true reason *does* satisfy the standing instruction;
what it must not do is resolve to silence.

Doing it turned up two defects in that reader. Every failure path collapsed to
the same empty list — a missing powershell, non-UTF-8 output, unparseable JSON
and a machine with genuinely no sensors were indistinguishable — so `note()` now
records which. And an unnamed sensor was being called `"Unknown"`, a literal that
reads as the sensor's actual name; those are skipped instead.

Item F is complete for every cluster **the plan named** — NUMA, RAPL, sensors,
virtualization and EDAC. That is not the same as every reader having a schema,
and an earlier version of this line said so, which was an overclaim.

### Probe before declaring: `examples/probe_readers.rs`

Adding clusters one at a time and guessing which readers answer was slow and got
one of them wrong. The example constructs every uncovered monitor and prints
what each produced, which turns "which cluster next" into a table. On this
Windows desktop:

| Answers here | Silent here |
|---|---|
| `input` 6, `services` 311, `storage_controller` 9, `power_profile` 3, `audio` 12, `bluetooth` 2, `camera` 2, `codec` 15, `printer` 4, `memory_bandwidth` 1, `memory_topology` 2, `cpu_microarch` 8, `crypto_accel` 11, `interconnect` 1 | `iommu`, `interrupt_map`, `io_scheduler`, `dma_engine`, `gpu_topology`, `thermal_zone`, `voltage_regulator`, `watchdog`, `security_mitigations`, `kernel_params` |

The silent column is almost entirely Linux sysfs. Those are not blocked in the
way the hardware items A–D are — a Linux box would let all nine be verified in
one sitting, and item F's remaining work is mostly that.

**Read the silent column carefully, because it was over-read once already.** The
probe originally printed `ok 0` for those nine, which says only that the monitor
constructed and enumerated nothing — not that this machine lacks the hardware.
This desktop certainly has an IOMMU and thermal zones. The table now prints
`none` for that case and `ok` only when something was enumerated, but the
underlying readers still return `Ok(vec![])` rather than an error carrying a
reason, so `none` remains ambiguous between "absent here" and "unimplemented
here". Establishing which, per reader, is the first step of that Linux sitting —
and giving each an error with a reason, as RAPL got, is the fix.

### Clusters added in the uncommitted batch

`memory.bandwidth.*`, `cpu.microarch.*`, `cpu.crypto.*` (features and RNG
sources), `board.input.*`, `board.audio.*`, `board.camera.*`, `system.printer.*`,
`network.bluetooth.*`, `disk.controller.*`, `power.profile.*`, `gpu.codec.*`.
Each has its own `<none>` diagnostic, per the sub-cluster rule above.

Three judgement calls in there worth preserving, because each is a place where
publishing *more* would have been publishing worse:

- **Scores were left out on purpose.** `cpu_microarch` computes a 0–100
  `single_thread_score` and `crypto_accel` an `acceleration_score`. Both are
  table lookups keyed on a name, not measurements of the processor in front of
  you. There is no provenance that fits: `Measured` lies about the method and
  `Derived` lies about the inputs. An agent choosing between machines on a
  number called a performance score would be trusting a guess wearing a
  benchmark's authority. The extension list beside it is the part that is a
  fact, and facts are what got published.
- **`gpu.codec.*` picks its provenance per row**, and it is the only cluster
  that does. The reader records whether it asked the driver (`DirectQuery`) or
  concluded the capability from the GPU model, so a queried row resolves
  `Measured` and an inferred one `Specification`. This is the clearest
  illustration in the crate of why the field exists — the two look identical
  until you ask, and only one survives a driver update.
- **Serial numbers and Bluetooth MACs are readable and are not published.** They
  identify a unit rather than describe it, no agent task needs one, and a
  hardware report carrying them is harder to share than one that does not.

### Five defects the batch turned up, all found by reading the output

None of these were found by a test. Every one came from running
`simon snapshot --format text` and looking at the rows.

1. **The existing `memory.dimm.*` cluster declared SMBIOS data as `Measured`.**
   A part number was not measured off the module — the firmware said so. Now
   `Specification` throughout, which is what that provenance is for. A board
   that lies about its own DIMMs makes simon repeat the lie, and a consumer
   deciding whether to trust the figure needs to know that before it decides.
2. **`cpu.microarch.name` emitted `{:?}` of an entire struct** — braces, nested
   quotes, `None` — into a reading declared as an identifier. It parsed as a
   value and was one only in the sense that a screenshot of a table is a table.
   Now split into `name`, `codename`, `vendor`, `isa`, `process`, `year`,
   `hybrid`.
3. **CPUID family/model/stepping published `0` as a reading** on Windows, where
   the triple is never decoded. Family 0 identifies no x86 part that has shipped,
   so it means "not read". All three now go together or not at all: stepping 0 is
   a legitimate value, and publishing it while family is absent would present a
   default as a measurement.
4. **The Windows camera reader enumerated scanners.** Its WMI query accepted
   PnP class `Image` as well as `Camera`, so a Brother MFC-L2900DW appeared as
   two cameras. That is not a slightly generous enumeration; it is a wrong answer
   to "can this machine see". Windows 10 1703 and later put every webcam under
   `Camera`, so narrowing the query loses nothing real.
5. **`disk.controller.pci_address` published device instance paths.** Windows
   returns things like `ROOT\SPACEPORT\0000`, which is not a PCI address and
   cannot be joined against `pci.*`. A consumer looking for a link width would
   have been sent to a bus the device is not on. `looks_like_pci_address` now
   gates it, and the absence names what was actually found.

That is five defects in one afternoon from one habit. **The conformance tests
catch what a value *is* — null, wrong JSON type, an absence with no reason. They
cannot catch what it *means*.** A debug dump is a non-empty string; a scanner is
a plausible camera name; a device path is a plausible identifier; zero is a
plausible integer. Read the rows.

### What is left

**`services` is done** — counts plus named failures, not an enumeration. See
`2687dbb`. **The nine silent readers now decline with reasons** rather than
returning an empty list; see `ebab956`. Verifying what they *read* still needs a
Linux box, but the Windows behaviour is now honest and was verified here.

Two readers remain, and neither is the simple schema job the old wording implied:

- **`interconnect` ships nothing, deliberately.** Decided rather than deferred.
  Its numbers come from `name.contains("GRANITE")` against a table of constants,
  and no provenance fits: `Measured` lies about the method, `Specification` lies
  about the inputs, because a spec sheet describes a part rather than a guess
  keyed on its marketing name. Publishing only the structural fields would be
  worse than publishing nothing — a legitimate-looking `type` and `generation`
  lend credibility to a cluster whose numbers are invented.
- **`kernel_params` is done**, as a split rather than a subset — `name`,
  `value`, `category` published; the scores and recommendations left to the
  tuning surface. See `014e4dd`.

  **It also found a guess inside the probe table.** `kernel_params` was counted
  by `|_| 1` — a literal — so the table reported "answers here, 1 item" for a
  reader that answers nothing on this machine, and the count propagated into the
  table above. The `Get-NetTCPSetting` cmdlet it depends on *fails* here, and the
  reader was pushing a parameter with an empty value regardless.

  That table exists because guessing which readers answer "was slow and got one
  of them wrong". Worth keeping in mind before trusting it again: two other
  readers had the same hardcoded count. **A measurement nobody checks is a
  constant, and a constant carries no information** — the same sentence this file
  already applies to a permanently-red pipeline, one more level down.

**Two of those three are traps, and "answers here" is what makes them look
safe.** The probe table says a reader produced rows; it does not say the rows
are readings. Both were checked before declaring anything, and neither is ready:

- **`interconnect` measures nothing at all.** `InterconnectMonitor::infer_topology`
  is a string match on the CPU marketing name — `name.contains("GRANITE")` —
  returning hardcoded constants for link type, width, `speed_gts` and
  coherence protocol, with `bandwidth_gbs` arithmetic over those constants and
  `latency_ns` documented in its own field comment as "estimated". Not one
  value comes from the hardware. It is the same defect as the withheld
  `single_thread_score`, except a field called `bandwidth_gbs` reads as a
  measurement in a way a field called `score` does not. If any of it is ever
  declared, the structural facts (type, generation, coherence protocol) are
  `Specification` at best, `sockets` and `is_numa` are the only genuinely
  measured fields, and the bandwidth and latency numbers should not be
  published in any provenance — there is no honest one.
- **`kernel_params` is half fact and half opinion.** `name` and `value` are real
  sysctl reads. `is_recommended`, `recommended`, `security_score`,
  `network_score` and `recommendations` are this crate's judgement about what
  the value ought to be — which is precisely what `simon tune`'s standing rule
  forbids publishing as fact ("a proposed value comes from what the driver
  declared, never from this crate"). Declare the first two; the rest belong to
  the tuning surface if anywhere.

The generalisable point, since this is the third time the same shape has come
up in this file: **a reader that returns rows on this machine has cleared the
availability bar and nothing else.** Whether each field is a reading is a
separate question, answered by reading the reader, and a plausible unit is not
evidence — `bandwidth_gbs`, `latency_ns` and `single_thread_score` all look
exactly like measurements at the call site.

Still uncovered and not probed: `wsl`, `drm_monitor`, `hardware_ai`, `scheduler`.

Add a cluster per change, verify each field resolves to a true provenance on the
machine at hand, and check the absent variant first — see open work 5 for why
that is the standing instruction.

**Deliberately not planned:** SMART failure thresholds on Windows. They come from
SMART READ THRESHOLDS, which has no `IOCTL_STORAGE_*` equivalent, so the only
route is an elevated pass-through — giving back exactly what 3.3.0 bought. The
drive's own `PredictFailure` verdict is the same judgement the thresholds would
have produced, and is already reported.

## How the ontology is put together

Three layers, added in 6.0.0, each answering a different question. Knowing which
layer a change belongs in saves rediscovering the split:

- **`Entity`** (`src/ontology/mod.rs`) — *what a value means.* Id, domain, kind,
  unit, declared provenance, nullability, prose. Templated ids use `{n}`.
- **`Capability`** (`src/ontology/capability.rs`) — *whether simon can produce it
  on this platform, on this build.* `Support` is `Implemented`, `Partial`,
  `Unimplemented` or `Unverified`, and the last three carry a reason. This is the
  layer that stops the docs from claiming a feature the build does not have.
- **`Vocabulary`** (`src/ontology/vocabulary.rs`) — *what values are possible.*
  Closed sets for provenance, verdict, severity, scan status and the rest, so an
  agent can enumerate the range without parsing prose.

`src/ontology/resolve.rs` produces `Reading`s against the entities;
`src/ontology/jsonld.rs` maps the result to JSON-LD with QUDT units, mapped only
where an exact equivalent exists rather than approximated.

**Write conformance tests against invariants, not against today's state.** Three
tests here had to be rewritten because they asserted a fact that was true when
written: one required every reading marked usable to actually resolve, which
failed on a GPU-less runner and again on a battery-less one; another asserted
that *all* detection capabilities were stranded without commands, and broke the
moment detection gained one. The durable forms are "all-or-nothing" and "these
two counts agree" — properties that survive the code getting better. A test that
fails when a gap is *closed* is a test that punishes progress.

## Guardrails, and why each exists

| Test | Catches |
|---|---|
| `tests/manifest_portability.rs` | A feature depending on a target-gated crate — the bug that meant the crate never built on Linux or macOS |
| CI job *Feature combinations* | A feature that does not build in isolation — found `cargo install` broken on Linux on its first run |
| `tests/macos_readers.rs` | macOS readings that are not plausible readings; runs on `macos-latest`, which is the only Mac this project has |
| `tests/documentation_links.rs` | Broken relative links; machine identifiers in docs; documented `simon …` commands that do not exist |
| `smart::tests::a_drive_with_no_readable_counters_is_not_graded_healthy` | Health graded from an empty scorecard |
| `smart::tests::a_self_reported_failure_survives_inference` | A drive predicting its own failure being scored back to `Good` from clean-looking counters |
| `disk::ata_smart::tests` | ATA structure misparses: bad checksum accepted, zero-filled buffer read as a clean drive, temperature taken from the full 48-bit raw, table truncated at the first gap |
| `tests/plausibility.rs` | Physically impossible readings |
| `tests/agentic_contract.rs` | Schema/resolver disagreement on the agent surface |
| `tests/ontology_conformance.rs` | Anything a new entity can get wrong: unreachable ids, absences with no reason, non-nullable nulls, values whose JSON type contradicts their unit, `Unknown` dressed as a measurement. Derives its cases from the ontology, so a domain added later is covered without editing it |
| `tests/capability_conformance.rs` | A declared capability with no command behind it, or a surface whose handler count and declaration disagree. Checked in both directions, so an implementation with no declaration fails too |
| `tests/vocabulary_conformance.rs` | A closed value set drifting from the enum it describes. Caught `Provenance::Reported` — a variant that does not exist — being written into a resolver twice |
| `tests/honesty.rs` | Absences without usable reasons, and readings claiming a provenance the resolver did not actually have |
| `tests/zero_constructors.rs` | A `new()` that fabricates a reading instead of constructing from the system. The list is empty and must stay empty |

**One gap in that table, found by reading declarations rather than by a test.**
Nothing compares a *reading's* provenance against its *entity's declared*
provenance. `gpu.codec.*` resolves per row — `Measured` where the driver was
queried, `Specification` where the capability was inferred from the GPU model —
while all five entities declared `Measured`, so the schema promised a live
observation for values that had never been observed. Fixed in `b2dd07b` by
declaring the weakest provenance a row can carry, which is the rule `Derived`
already follows for its inputs.

The class is still uncaught. `Entity` holds one provenance and cannot say
"varies per row", and a `Varies` variant is not the answer — it would poison
`is_observation()`, the guard an agent applies before treating a value as fact,
for every consumer of every entity. The test worth writing asserts the safe
direction rather than equality: **a reading may resolve stronger than its
declaration, never weaker.** `honesty.rs` is where it belongs.

**A flaky test is the same instrument as a red pipeline.**
`profile::cache::tests::invalidate_forces_refresh` asserted on `CACHE_STATS`, a
process-global, while a sibling test reset it — so it failed on thread
scheduling, passed single-threaded, passed three times in isolation, and only
failed under the full 825-test parallel load. That profile is exactly what makes
one get dismissed as noise, and this project has already lost time to
"an intermittent failure at three runs in five". Fixed in `2e6f9e9`. When a test
touches a global, every test in the module that moves that global has to
serialise, including the ones that assert nothing about it.

Each was checked against a deliberate break before being kept, because a test
that cannot fail is worse than none.

**A green suite is evidence about the surface it covers, and nothing else.**
The Dewey port passed 839 tests through four releases while its window opened at
800x600 titled "Dewey App", clipped every value off the right edge, responded to
no clicks at all, and reported no GPUs on a machine with three. The tests
asserted that each tab emitted named ontology nodes; the port did that
faithfully. Nothing failed because nothing was asked. The bugs were found by
screenshotting the window and looking at it — and each round of looking found
another one. If a change affects something a person sees, look at it before
reporting it done.

## Two things worth knowing

**Don't run `cargo clippy --fix` per target in this crate.** Fixing for macOS
deleted `mut` and imports that only Linux and Windows need, silently breaking
both (21 and 14 errors). A `cfg` block that is empty on the target being fixed
makes bindings look unused. Six files had to be reverted.

**Whether a Windows ioctl needs elevation is written in its `CTL_CODE`.** The
access bits in the definition, and the mask the handle was opened with, decide it
— not how privileged the operation sounds. Two capabilities were scoped as
"needs Administrator" on the second reading and turned out not to (NVMe in 3.1.0,
ATA in 3.3.0), and one was scoped as reachable and is not
(`IOCTL_ATA_PASS_THROUGH`). Reading the `CTL_CODE` first would have settled all
three in a minute each.

**The absence-word guard lives in one place, and that is why it only had to be
written once.** `push_text`, `push_id` and `push_opt` in `resolve.rs` all funnel
through `push_str_as`, which rejects empty strings and the words "unknown",
"unspecified", "undetermined", "n/a" and "none". An enum's `Unknown` variant
lowercased into a measured identifier is the single most repeated defect in this
crate — it has been caught in five entities, in three different readers, across
three releases. Push string readings through the helpers, never
`Reading::measured` directly.

**A manual grep is not a substitute for asking the binary.** A hand sweep for
documented-but-nonexistent commands missed 29 of 38 cases; a test comparing docs
against `simon describe --commands` found them all, including a `CLI.md` at the
repo root nobody had thought to include. The same lesson produced the feature
sweep above: the `cli` breakage was found by running every combination, not by
reading the manifest.

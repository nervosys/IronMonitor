# Changelog

All notable changes to IronMonitor will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [6.0.0] - 2026-08-19

### Added

- **`ironmon status`** — a one-screen summary of the machine, terse by default and
  `--ascii` for the OS logo in the neofetch style, colourised when stdout is a
  terminal. It reads through the same resolver as `snapshot`, so the two cannot
  disagree about one machine.

  Anything unreadable prints the reason rather than a dash. "No GPU detected"
  and "GPU enumeration failed" call for different responses, and this is the
  surface people paste into issues.

- **`ironmon snapshot --format json-ld`** — a linked-data hardware report. The
  existing JSON says what the readings are and not what any of the words mean;
  the `@context` resolves ironmon's terms to IRIs and its units to QUDT, so an
  agent that has never seen IronMonitor can interpret the graph.

  Units map to QUDT where a mapping exists and not where it does not:
  `identifier` and `text` are not quantities. A wrong `@type` is a
  machine-readable claim that a GUID is a measurable amount of something.
  Absence survives the encoding — an unavailable reading is a node with no
  value and a `ironmon:unavailableReason`, never a zero or a null.

- **`ironmon ids`** — the intrusion detectors, which had no command until the
  capability catalogue was joined to the command list and reported that they
  were reachable from the library and from nothing typeable.

- **Package energy and platform sensors in the ontology.**
  `power.rapl.{n}.*` and `board.sensor.{n}.*`, each with a `<none>` diagnostic
  carrying why when the cluster is empty. Both readers previously returned an
  empty list where a reason existed only in a source comment.

- **Four system entities that were read and never named**:
  `system.hostname`, `system.os.version`, `system.kernel.version` and
  `system.architecture`. `os_info` had them all along; nothing exposed them.

- **A capability and vocabulary ontology.** `ontology::capability` declares each
  surface and platform's level of support — including `unverified`, for code
  that compiles and has never run on hardware. `ontology::vocabulary` declares
  the closed value sets an agent parses, so `{"status":"no_baseline"}` can be
  handled exhaustively. Both are cross-checked against the code by tests derived
  from the declarations rather than written beside them.

### Fixed

- **Three live defects found by renaming the zero-constructors.** The rename was
  queued as cosmetic. It was not: `CpuStats::new()` and friends returned
  fabricated numbers behind a name that reads like a reader, and going to change
  it turned up three places that had been believing them.

  - **`IronMonitor::snapshot_cpu` and `snapshot_memory` returned zeros.**
    Public API, documented as "snapshot current statistics", implemented as the
    zero-constructor. Every library consumer calling them received 100% idle,
    no cores, and no memory. They now read the platform: 24 cores and 76.7% idle
    on the development machine where they previously reported nothing.
  - **Health checks could never fire.** `SystemHealth::check` computed CPU usage
    as `100 - idle` from the zero-constructor, so it always saw 0% and no
    threshold could be crossed. It now reads: 23.4% CPU, 51.4% memory, and it
    correctly reports elevated swap as a warning.
  - **The Prometheus exporter published 0% CPU on every scrape.** Same
    arithmetic, same cause, in the integration whose entire purpose is to be
    believed by a time-series database.

  This is the third, fourth and fifth defect from this one pattern; two GUI
  cases and one HTTP-server case were found earlier. The name was doing the
  damage, so the name is gone.

### Changed

- **BREAKING: `CpuStats::new()`, `MemoryStats::new()`, `PowerStats::new()` and
  `ProcessStats::new()` are now `empty()` and return `Self` rather than
  `Result<Self>`.** Building a zeroed struct cannot fail, and the `Result` was
  inviting `?` at call sites that then read like they had gathered something.

  `empty()` says what it does. Callers wanting a reading want the per-platform
  readers, or `IronMonitor::snapshot_cpu` and `snapshot_memory`, which now
  actually read.

  `tests/zero_constructors.rs` still runs, and its list of known fabricators is
  now empty: no `new()` in this crate returns a fabricated reading. Adding one
  fails the build.

## [5.2.0] - 2026-08-17

### Added

- **The tuning loop is closed: `ironmon tune` can now measure whether a setting
  helped, and undo it if it did not.** `tuning::verify` measures a metric before
  a write, waits a settle period, measures again, and reverts on a demonstrated
  regression. `serve::cycle_verified` runs a tuning pass that way, and each
  `AppliedOutcome` carries the `Verdict`.

  5.1.0 made writes reversible but left nothing to decide *whether* to reverse
  them. A tuner that can apply and undo without telling the two apart is still
  guessing, it just guesses in both directions.

  **The registry of metrics is deliberately empty, and that is the feature.**
  Declaring a metric is a claim that a particular number moves when a particular
  setting changes, and the obvious candidate did not survive being checked: the
  natural metric for a CPU power-scheme change is achieved clock speed, and on
  Windows `CallNtPowerInformation(ProcessorInformation)` reports a nominal one.
  Measured here — 16 spinning threads took system idle from 79.7% to 11.4% and
  every core reported exactly 4400 MHz throughout, before and after. A verifier
  built on it would have said "no change" for every power scheme in existence
  and been believed. So `active_scheme_guid` has no metric, and the honest
  output is `unverifiable`.

  Consequently `Verdict::Unverifiable` is the default outcome rather than the
  exception, and is kept distinct from `Unchanged`: "we looked and could not
  tell" is not "we looked and found nothing". `AppliedOutcome.verdict` being
  `None` is a third fact again — nobody looked, because the cycle was not a
  verifying one.

  Only a measured regression triggers a revert. Not `Unchanged`, and not
  `Unverifiable` — undoing on "I could not tell" would reverse nearly every
  write this crate makes, which is a way of ignoring the measurement rather than
  a safer use of it. Noise is handled by comparing medians of sampled windows
  against a threshold that is the larger of the metric's declared minimum effect
  and the scatter the two windows actually showed, so a jittery metric raises
  its own bar instead of producing confident verdicts from noise.

  `revert_cycle` skips writes that verification already undid; reverting a
  revert would restore the value the loop had just measured as worse.


- **macOS CPU and memory now resolve in the ontology.** `read_cpu_stats` and
  `read_memory_stats` in the resolver had no macOS arm: they returned `None`
  unconditionally, and the resolver then reported "the platform CPU reader
  returned an error" — which was false, since no reader had been called.

  Eight entities were affected. `cpu.total.utilization`, `memory.total`,
  `memory.used` and `memory.utilization` carried that fabricated cause;
  `cpu.model`, `cpu.cores.logical`, `cpu.cores.physical` and `cpu.total.idle`
  came back as "no resolver bound on this build", all four of them declared
  non-nullable, so the schema was promising values it never delivered.

  Everything needed was already in `platform::macos` and unused — `per_core_ticks`
  (the only source of nice time), `aggregate_ticks`, `vm_stat`, `sysctl_u64`,
  `swap_usage`. This adds the two adapters that assemble them and wires both
  resolvers. Where macOS genuinely has nothing it says so: `governor` is left
  empty, which the resolver reports as unavailable with a reason, and per-core
  frequency is `None` rather than a nominal figure — the mistake Windows makes
  with `CurrentMhz`, documented in `tuning::verify`.

  Verified on `macos-latest` in CI, which is the first time these readers have
  executed at all.

- **`ironmon ai models` reports what is in an IronVault model vault**, behind a
  new `vault` feature that is **off by default**. It lists each stored model's
  name, format, version count, stored and compressed size, checksum and
  metadata, as a table or as JSON.

  The integration is deliberately read-only. IronMonitor does not store, fetch,
  decrypt or delete anything; it reads the vault's metadata index, which
  IronVault exposes without a passphrase, and reports it. A locked vault is
  still fully reportable — verified against a real vault holding two models
  across three versions, with `"unlocked": false` in the JSON output.

  **Reading a vault leaves no trace.** `security.audit_log` is disabled for the
  read, and the vault directory is byte-identical before and after: verified by
  comparing the full file listing across two consecutive reads.

  `VaultStatus` keeps the outcomes that this crate's ontology requires to stay
  distinct once serialised — `not_installed` (no IronVault on this machine),
  `absent` (IronVault is present, this path holds no vault), `present`, and
  `failed` with a reason. Collapsing the first two would report an empty path
  as though a vault had been looked for and not found.

### Fixed

- **CI had been failing on every run for over a week, across two published
  releases, and is green again.** Twelve consecutive red runs spanning 5.0.0 and
  5.1.0. That is why the `cli` break below shipped: the job written to catch it
  was reporting the fault the whole time, and a pipeline that is always red
  reports nothing anyone reads.

  Seven defects, each hidden behind the one before it because a failing job
  cancels the rest:

  - Three clippy errors in `cfg(target_os = "linux")` code, which a Windows
    machine never compiles.
  - `revert_without_confirmation_is_refused` asserted `NeedsConfirm` for
    `active_scheme_guid`, a Windows-only setting. macOS registers no writable
    settings, so the refusal correctly comes back as `NotWritable` from the
    handler lookup. The test now asserts the invariant that holds everywhere —
    an unconfirmed revert never writes — then the platform-appropriate refusal,
    asking the handler registry which case applies rather than restating its
    `cfg`s.
  - `every_documented_command_exists` read a sentence in `HANDOFF.md` beginning
    "IronMonitor already detects…" as a command. Self-inflicted, by pushing a
    docs-only change without re-running the suite.
  - `resolve_gpu` reported all three of its failure paths as an unavailable
    `gpu.0.name`, asserting an adapter exists that could not be read when the
    truth is none is known to exist. They now use `gpu.<none>`, the declared
    per-domain diagnostic that already existed for exactly this.
  - `board.firmware.{n}.version` resolved to the literal `"n/a"` with `measured`
    provenance on virtualised firmware.
  - `diagnostics_do_not_contradict_the_readings_beside_them` counted every row
    in a domain, including unavailable ones that assert nothing, so
    `gpu.<none>` beside three unread GPU settings looked like a contradiction.
    It now counts observations, as its own documentation always described.
  - `pci.*.link.speed` resolved to `"unknown"` with `measured` provenance.

  Four of those seven are the same fault — an absence dressed as a reading —
  which is the one thing this crate's ontology exists to prevent. It is now
  caught by a final pass over the assembled snapshot, modelled on the range gate
  beside it. Guarding the individual push helpers was tried first and failed
  twice, because it requires knowing the complete set of helpers and that guess
  was wrong each time.

  **The lesson worth keeping is about the signal, not the bugs.** A red pipeline
  nobody reads and a green one that checks nothing fail in exactly the same way:
  they return the same value regardless of input. This project has now hit that
  three times — 839 passing tests over a GUI that reported no GPUs on a
  three-GPU machine, a test failing three runs in five and read as noise, and
  this.

- **`--no-default-features --features cli` builds again.**
  `handle_gui_frame_command` names `ironmonlib::gui` and was not behind
  `#[cfg(feature = "gui")]`, though its only caller was. The omission broke
  nothing at runtime and broke the `cli` feature completely.

  This is the same failure the CI feature-isolation job was written for after
  3.0.0 — a feature that only builds because some other feature supplies what it
  is missing — recurring after the 5.0.0 GUI restore, which means it has been
  live since then. All 19 advertised features plus the no-feature build are now
  verified to compile in isolation.

- **The Peripherals tab no longer waits 24 seconds for data it does not show.**
  System info was collected by one background thread that ran seven WMI queries
  and sent nothing until all seven finished. Two of them dominate: measured here,
  `get_driver_versions` takes 20.3s and `get_system_temperatures` 2.9s, against
  0.7s for `get_peripherals` itself. The Peripherals tab paints USB and PCIe and
  no driver table at all, so it sat on a spinner for the duration.

  Headless reads gave up at 30s and returned a bare title. Intermittently:
  `ironmon gui --frame --tab peripherals` produced nothing usable in **three runs
  out of five**, and `every_gui_tab_paints_text` failed at the same rate. It now
  passes ten consecutive runs, and the tab returns in 8.8s with 192 lines of
  content instead of two.

  The collector sends two instalments, split by cost rather than by tab, and the
  two duplicated copies of the thread body — which had already drifted by one
  `sleep` — are now one function. The System Info tab still waits for the slow
  queries, because it genuinely displays them.

  This was live in every release since the tab existed, and the test that should
  have caught it did catch it — just not reliably enough for anyone to believe
  it. An intermittent failure that nobody chases is a passing test.

- **The declared minimum Rust version was wrong, and is now 1.88.** The manifest
  said 1.70. Building against every installed toolchain shows the crate has not
  compiled on 1.70 since the egui GUI was restored in 5.0.0: `eframe` and
  `egui_extras` pull in `image 0.25.10` and `home 0.5.12`, which declare 1.88
  themselves, and `image` reaches an edition-2024 crate (`pxfm`) that pre-1.85
  cargo cannot even parse. 5.0.0 and 5.1.0 both shipped with a `rust-version`
  that nothing built against and no CI job checked.

- **`virtualization::detect::cpuid` compiles on 1.88 again.** The `unsafe` block
  around `__cpuid` had been removed as a no-op — true on current Rust, where the
  intrinsic is safe to call, but on 1.90 and earlier it is still `unsafe` and the
  crate did not build at all. It is restored under `#[allow(unused_unsafe)]`,
  which satisfies both. The same edit also splits the function per architecture:
  it previously named `core::arch::x86_64` in its return type while being gated
  on `any(x86, x86_64)`, so a 32-bit x86 build could never have compiled.

### Notes

- The `vault` feature raises the required Rust version to **1.89** for builds
  that enable it, because that is IronVault 7's MSRV — verified by building with
  and without the feature on 1.88 and 1.89. ironmon's own floor stays at **1.88**,
  which is why the dependency is optional rather than default: the 4.0.0 release
  raised the floor for a GUI dependency and that was a mistake worth not
  repeating.

- **Upstream finding, IronVault 7.** `VaultConfig::new()` has side effects — it
  creates the config, data and cache directories and writes a `config.yaml`
  before anything has been read. There is no non-creating path resolver, so a
  read-only caller cannot ask "where would the vault be?" without bringing one
  into existence. ironmon works around this with an `installed()` probe that
  checks for the directories itself and refuses to construct a config when they
  are missing. If IronVault gains a resolver that does not write, that probe
  should be deleted.

## [5.1.0] - 2026-08-12

### Added

- **Applied settings are reversible.** `ApplyHandler::read_current` reads a
  setting before it is written, `ApplyOutcome.previous` records what was
  overwritten, `profile::apply::revert_setting` puts it back, and
  `tuning::serve::revert_cycle` undoes everything one tuning cycle applied, in
  reverse order.

  Until now `ironmon tune --apply` could change a machine and had no way to change
  it back: the handler trait could only write, and an outcome recorded only what
  was requested. An autonomous tuner that can move a machine in one direction
  only is not something to leave running unattended.

  A revert goes through `apply_setting`, so it is confirmed and audit-logged on
  exactly the same terms as the write it undoes. Where no prior value was
  recorded — because the handler could not read the setting — revert refuses
  rather than writing a default: putting a machine into a state it was never in
  is a worse failure than leaving it where the caller put it.

  `read_current` is implemented for the Windows active power scheme
  (`PowerGetActiveScheme`, verified against `ironmon profile explain
  active_scheme_guid`, which reaches the same value through unrelated code) and
  for the Linux cpufreq governor. The Linux path is written by inspection and has
  not been run.

- **Unattended writes must be reversible.** `profile::apply::apply_setting_reversible`
  refuses when the value currently in effect cannot be read, and the automatic
  tuning server uses it. `apply_setting` still writes in that case, for attended
  callers who can be told "this cannot be undone" and decide for themselves.

- **A failed read is retried before a setting is declared unreadable.** Observed
  during testing: `PowerGetActiveScheme` failed once on a machine running several
  compiles, then read normally. Without a retry a transient failure silently
  converts a reversible write into a one-way one — the write succeeds, no prior
  value is recorded, and nothing says so until someone tries to undo it.

### Changed

- `ApplyOutcome` gains a `previous` field and derives `PartialEq`. The field is
  `#[serde(default)]`, so audit records written before 5.1.0 still deserialise.

## [5.0.0] - 2026-08-12

### Changed

- **The Dewey GUI is withdrawn and the egui GUI is restored.** 4.0.0 replaced a
  ~10,000-line egui application with a ~2,100-line Dewey one, and 4.0.1 through
  4.0.4 spent four releases trying to make the replacement presentable. It never
  got close. Running the two side by side is the clearest statement of why: the
  original shows nine metric cards including live GPU temperatures, six
  sparkline charts with axes and min/max, a threshold legend, an uptime and task
  status line, an AI ask bar, and JSON/CSV export — and it detects three NVIDIA
  GPUs on a machine where the Dewey port reported
  `Accelerators: unavailable — Failed to initialize COM`.

  The port was not only less finished to look at. It was reading less hardware,
  and the tests said nothing about either fact: they asserted that each tab
  emitted named nodes, which the port did faithfully while showing a fraction of
  what it replaced.

BREAKING CHANGE: `deweygui` is no longer a dependency, the `dewey-gui` feature
and the `ironmonlib::gui_dewey` module are removed, and `gui --script` speaks
ironmon's own `goto` / `assert` / `capture` vocabulary again rather than Dewey's
JSON agent protocol. `ironmonlib::gui::headless` and `gui::app::IronMonitorApp`
are back. MSRV returns to 1.70 — Dewey's edition 2024 was the only thing that
required 1.85.

### Known issues carried back

- The headless path keeps its 30-second deadline and per-tab settle predicate.
  Avoiding that machinery was the argument for the migration, and it is the price
  of this reversal.

## [4.0.4] - 2026-08-12

### Changed

- **The Overview is rebuilt from the egui GUI's own widgets**, read out of git
  rather than approximated. 4.0.3 had the original palette on a layout I had
  invented, which is why it still did not look like ironmon.

  - The Glances-style **QuickLook strip**: four labelled mini-bars across a 32px
    surface panel, threshold-coloured, with the reading beside each. This sat at
    the top of the original Overview and is the most recognisable thing about
    it.
  - **Section headers**: a 14px cyan title followed by a hairline rule to the
    right edge. The rule is what made a ironmon pane identifiable at a glance, and
    4.0.0 had dropped it entirely.
  - **Metric cards** at their original 140x70 proportions -- a 3px accent stripe
    down the left edge in the device colour, an 11px secondary title, an 18px
    value in that colour. The 4.0.3 tiles were three times the size with a 30px
    number and no stripe.
  - A dense two-column process table, split evenly, the way htop fills a
    terminal rather than leaving half the pane empty.

### Fixed

- QuickLook draws no bar for counts. TASKS and DISKS are not percentages, and a
  bar at 0% reads as "nothing" — the opposite of "32 tasks".

## [4.0.3] - 2026-08-11

### Changed

- **The GUI is a dashboard rather than a list of readings.** Overview is a row
  of stat tiles -- a small tinted label, the number at 30px, a supporting line
  and a load bar -- over two panels holding the process table and what is
  attached to the machine. The previous pane spelled every reading as
  `label: value` at one size and one weight, so nothing was findable without
  reading all of it.

- **Colour is reserved for things worth looking at.** Values stay neutral until
  they cross 60%, then go amber and red. Domain tints sit on section labels and
  load bars, not on every row. Colouring everything by status had turned a
  healthy machine into twenty green rows all saying "fine", which is the same as
  saying nothing.

- Near-black ground with lifted cards, hairline borders and a 12px radius, one
  accent for the active tab, 20px padding throughout, and 13px rows.

- **The original `CyberColors` palette is back**, recovered from git rather than
  reinvented: GitHub-dark grounds (13,17,23 / 22,27,34 / 30,37,46) under the
  device-class neons the TUI also uses -- cyan CPU, green accelerators, magenta
  memory, orange disk, blue network. Section titles carry their device colour
  again, and stat tiles show their number in it. 4.0.0 had replaced all of this
  with a neutral palette of my own, which is the main reason the port stopped
  looking like ironmon.

- Threshold bands are the original four at 90 / 70 / 50, not the three at 85 /
  60 the rewrite used, so a machine at 55% reads cyan again rather than green.

### Fixed

- Stat tiles clamp their supporting line. A provider failure --
  `Failed to initialize COM: HRESULT ...` -- is longer than the card and ran out
  past its border.

## [4.0.2] - 2026-08-11

### Fixed

- **The GUI window was barely usable, and the tests could not see it.** The
  window opened at Dewey's default 800x600 titled "Dewey App", because
  `Model::title` and `ProgramOptions` were both left at their defaults. At that
  width every reading fell off the right edge, so the first pane showed a column
  of grey labels with no values beside them. `gui --frame` could not catch this:
  it renders through `TestBackend` into a fixed 1280x800 area that no real
  window has.

- **Nothing in the GUI was clickable.** `handle_event` matched the Tab and 'r'
  keys and had no `Event::Mouse` arm at all. Clicks now select tabs, and number
  keys jump straight to one.

- **Clicks landed on the wrong tab.** Dewey's `Tabs` sizes each tab to its label
  rather than to an equal share of the bar, so equal-width arithmetic selected
  the neighbour. `view` now measures the same way the widget does and records
  the spans for `handle_event`.

- **The selected tab was invisible.** `Tabs` fills the active tab with
  `style.background`, which had been set to the bar's own background colour.

- **Progress bar labels were unreadable.** Dewey paints them in the style's
  foreground — the same colour as the fill — so the text vanished as the bar
  filled, exactly when it was worth reading. Readings now sit in their own row.

### Changed

- The GUI has a visual design: a colour per domain, green/amber/red thresholds,
  two-column rows, and panes on cards. Failures read as failures — red for
  unavailable, grey for loading — so neither can be mistaken for a measurement.
  This carries the intent of the deleted `CyberColors`, not its 1,167 lines of
  custom widgets; the gauges and sparkline treatments are not back.

- Overview fills the window: load bars, the three busiest cores, top processes,
  attached storage, and the busiest network interfaces.

## [4.0.1] - 2026-08-11

### Fixed

- **The GUI violated the Vulkan spec on every frame.** Running `ironmon gui` under
  a Vulkan validation layer produced
  `VUID-vkQueueSubmit-pSignalSemaphores-00067` continuously: a binary semaphore
  still bound to a pending presentation was being resubmitted, because wgpu's
  Vulkan surface path reuses one semaphore across swapchain images rather than
  one per image. It rendered, but undefined behaviour that happens to work on
  this driver is not the same as working.

  The fault is wgpu's, beneath both this crate and Dewey — the chain is Dewey →
  eframe → egui-wgpu → wgpu, and Dewey's egui path contains no semaphore or
  swapchain code of its own. `wgpu_hal::vulkan` appears in the log because that
  is where the validation callback is installed, not because it authored the
  mistake.

  The GUI now runs on Dewey's `agpu` backend, which manages its own surface
  semaphores and already carries a workaround for a related wgpu 24.x cleanup
  panic. Measured after the switch: no validation output at all during rendering,
  and a clean exit on `WM_CLOSE` — teardown being the specific moment that other
  wgpu bug strikes.

  Only `gui::run` changes. `gui::frame` and `gui::script` render through Dewey's
  `TestBackend` and touch no GPU, which is also why the full 839-test suite
  passed without a hint of this: the interactive surface is not the one the tests
  cover.

## [4.0.0] - 2026-08-11

The GUI is now built on [Dewey](https://crates.io/crates/deweygui), nervosys'
agentic-first GUI framework. The ~10,000-line immediate-mode egui implementation
is gone.

### Breaking

- **MSRV is 1.85**, up from 1.70. Dewey is edition 2024. Cargo resolves
  `rust-version` per crate rather than per feature, so this applies even to
  builds that do not enable `gui`.
- **`ironmonlib::gui::headless` is removed.** Its job — render a tab without a
  display — is now `gui::frame`, which returns the ontology tree as JSON rather
  than painted text.
- **`ironmonlib::gui::app::IronMonitorApp` is removed**, along with the rest of
  the egui implementation. `gui::run`, `gui::frame` and `gui::script` are the
  public surface.
- **`gui::run` returns `Box<dyn Error>`** instead of `eframe::Error`.
- **`gui --script` speaks Dewey's agent protocol** — one JSON request per line —
  instead of the bespoke command vocabulary IronMonitor had invented.

### Why

3.9.0 fixed a bug where four tabs rendered only a spinner under `gui --frame`.
Their contents loaded on background threads collected by
`check_background_loaders`, which runs only inside the interactive event loop, so
the headless path never collected them. The fix taught the headless path to pump
the loaders: a 30-second deadline, a per-tab settle predicate combining loader
flags with painted text, and two wrong attempts before it worked.

Dewey's answer is that the loaders were never the app's to pump. Background work
is a `Command::Task` the runtime owns and delivers as an ordinary message. There
is no "collect the loaders" step to forget, so there is no
headless-versus-interactive divergence to get wrong. The bug class is absent
rather than fixed.

The same applies to how the GUI is read. Every widget carries an `agent_id` and a
headless read returns named ontology nodes, so a test asks whether
`memory_total` is present instead of whether *some* text was painted — which a
spinner satisfies, and which is why four broken tabs passed the contract test for
six releases.

### Added

- All thirteen tabs render under Dewey: Overview, CPU, Accelerators, Processes,
  Memory, Network, Disk, System, Peripherals, Profiles, Connections, Network
  Tools, AI Assistant. `every_egui_tab_has_a_dewey_counterpart` asserts the count.
- `gui::frame(Some(tab))` renders any tab headlessly and returns its ontology
  tree. No deadline, no settle predicate: `init()` runs the loaders to completion
  because the runtime owns them.
- CPU history chart, replacing the `egui_plot` graph. It draws only from two
  samples on, since a one-point line chart is a dot implying a trend it cannot
  have.

### Fixed

- **The GUI showed 0 MB of memory and a fully idle CPU with no cores on Linux.**
  `MemoryStats::new()` and `CpuStats::new()` are zero-constructors, not readers.
  The egui GUI called the real per-platform readers only under
  `#[cfg(target_os = "windows")]` and fell back to the zero-constructors
  everywhere else. Both now call the Linux readers.

  `SystemStats::new()` in the same crate does the right thing and dispatches to
  the platform reader. The other two cannot follow it —
  `platform::linux::memory::read_memory_stats` calls `MemoryStats::new()?` as its
  starting struct, so making `new()` read would recurse forever. They are builder
  bases wearing a constructor's name; `empty()` would have prevented both defects.
  Renaming them is recorded as open work.

### Behaviour worth knowing

- **Network Tools runs nothing at load.** Ping, traceroute and port scans send
  packets to hosts the user names, so they now require an explicit action, and
  the pane says so on screen. `network_tools_runs_nothing_on_load` asserts the
  negative — the kind of guarantee that is otherwise lost silently, since a
  version that pinged on load would still render a pane and every other test
  would pass.
- **The AI pane does not probe backends at load.** Probing is a network call, and
  blocking a frame on one makes reading the GUI as slow as the slowest
  unreachable host.
- **Peripherals omits MAC and Bluetooth addresses, and System omits the serial
  number and machine UUID.** These panes are read by agents and pasted into
  issues. Connections *does* show remote addresses — they are the substance of
  that tab.
- Row caps are applied by loaders, not views, so the model states exactly what a
  tab can show and a headless read is not misled into thinking it received a
  complete table.

## [3.10.0] - 2026-08-10

A bare-metal Windows 11 desktop is no longer reported as a virtual machine.

### Fixed

- **`is_virtual_machine()` returned true on a Hyper-V root partition.** Windows 11
  enables virtualization-based security by default, which puts the host under a
  thin hypervisor: a physical desktop reports the "Microsoft Hv" CPUID signature
  exactly as a guest VM does. Every caller that read the vendor string and
  stopped there — `is_virtual_machine()`, `detect_platform()`, and the
  `system.virtualization.platform` ontology entity — has been wrong on ordinary
  Windows 11 hardware since virtualization detection was added.

  `hyperv_partition()` now reads the partition privilege mask from Hyper-V CPUID
  leaf 0x40000003. `CreatePartitions` (EBX bit 0) and `CpuManagement` (EBX bit 12)
  are root-only: the root partition is what creates and schedules guests, so a
  guest is never granted them. Both are required rather than either — one bit is
  a thinner reed than the pair, and they are set together on a root partition.

  Measured on this desktop: `ebx=0x002bb9ff`, with CreatePartitions,
  AccessPartitionId and CpuManagement all set. `ironmon get
  system.virtualization.platform` now answers `bare_metal [measured]`.

- **The ontology's 3.7.0 workaround is withdrawn.** That release reported
  `system.virtualization.platform` as *unavailable* rather than guess, on the
  grounds that this is the entity an agent consults before trusting every other
  reading and so the worst possible place for one. The entity now resolves as a
  measurement, which is what the workaround was waiting for.

- **All three "am I virtualized" paths route through one helper.**
  `hypervisor_indicates_vm()` is the single place the root-partition case is
  handled; `detect_platform()` had the identical defect on the Windows, macOS and
  Linux arms and was silently fixed by the same change.

### Verification

The root-partition side is measured here. The **guest** side is not verified
against a real Hyper-V VM — it follows from the TLFS privilege definitions, and
if it is wrong the failure mode is the status quo ante, a guest misreported as a
host. The new `partition_agrees_with_hypervisor_vendor` test deliberately does
not assert `Root`: it has to hold both on this desktop and in CI, where the
Windows runners are themselves Hyper-V guests on Azure — which is what exercises
the `Guest` arm no hardware here can reach.

## [3.9.0] - 2026-08-10

The headless GUI surface could not read four of its nine tabs.

### Fixed

- **`gui --frame` and `gui --script` rendered only a spinner for the disk,
  system, peripherals and profiles tabs.** Those four load their contents on a
  background thread, and the results are collected by `check_background_loaders`,
  which only runs inside the interactive event loop. The headless path draws a
  tab directly, so nothing ever collected them and the frame said
  "Loading disk information…" forever.

  It went unnoticed because a spinner *is* painted text, so
  `every_gui_tab_paints_text` passed. The four affected tabs are the ones
  carrying the SMART, PCI and USB work of the preceding six releases — an agent
  reading the GUI could see none of it.

  Headless reads now pump the loaders between frames until the tab settles or 30
  seconds pass, and report on stderr when the deadline was hit, so a slow read is
  distinguishable from a stuck one. All nine tabs render data: disk 6.7 s,
  peripherals 16.5 s (its loader runs several PowerShell CIM queries), the rest
  under 4 s.

- **`every_gui_tab_paints_text` now asserts a tab painted something besides
  placeholders**, which is the property that was actually wanted. Two earlier
  formulations were wrong: forbidding any placeholder failed a system tab that
  legitimately shows data while one section arrives, and requiring more than two
  substantive lines failed the AI tab, whose backend probe is deliberately not
  waited on — blocking every headless read on a network call would make reading
  the GUI as slow as a DNS timeout.

- **The settle predicate is per tab, not global.** A first version waited on
  every loader for every tab, which took the memory and network tabs from instant
  to twelve seconds waiting on a peripherals query they do not draw. It now
  consults only the current tab's loaders, combined with a check of the painted
  text — both are needed, because the disk tab has a third state between spinner
  and data ("No Disks Detected") that text alone cannot tell from a machine that
  genuinely has none.

- **`coverage_accounts_for_every_reading` was racy.** It compared `coverage()`
  against a second, independent `snapshot()` and asserted equal lengths — but
  instance counts move between calls on a live machine, so it was passing only
  while the box was quiet and failed once the suite got busier. `coverage_of`
  now tallies a snapshot the caller already holds.

## [3.8.0] - 2026-08-10

Use-case detection and hardware profile recommendations, with an automatic
server. AI and gaming are the first cases covered.

### Added

- **`ironmon tune`** — detects what the machine is being used for (AI training, AI
  inference, gaming, interactive, idle) and recommends the profile settings that
  suit it. `--watch N` turns it into the automatic server, re-evaluating every N
  seconds. `-f json` for the machine-facing form.

  **It recommends and writes nothing by default.** Applying needs `--apply` *and*
  `--confirm`, and every write goes through `profile::apply::apply_setting`,
  which refuses without confirmation and writes an audit record — the contract
  AGENTS.md states, which a server that quietly rewrote power settings would have
  made false.

- **Proposed values come from the hardware, never from a model.** This is the
  central constraint of the design. `Recommendation::basis` records where each
  value came from — an entry in the setting's own driver-declared choice list, or
  its reported default — and a setting that enumerates no choices is *skipped
  with a reason* rather than given a plausible-looking GUID or governor name.
  `tuning::tests::a_recommendation_never_proposes_a_value_the_driver_did_not_offer`
  makes that enforceable.

  The tempting design is to ask a language model for a power limit. That produces
  a number with no provenance that cannot be checked against anything the
  hardware said — the failure this repository's ontology exists to prevent, and
  worse here than in a reading, because a reading is only believed while a
  setting is *written*. A model may classify the workload, where being wrong
  costs a suboptimal profile. It may not choose values.

- **A local model classifies where one is running.** `tuning::classify` asks
  Ollama or LM Studio to pick among the `UseCase` variants, given the same signal
  summary the heuristics see, and parses the answer through `UseCase::parse`.
  Anything unrecognised is discarded and the deterministic path answers instead —
  the model cannot widen the answer space, only choose within it, so the worst
  outcome is a wrong label rather than an unknown one. Only local backends are
  considered: a description of what someone is doing at their desk is not
  something to post to a hosted provider silently. Every fallback records its
  reason in the evidence, so "no model was involved" is never silent.

- **Classification carries its evidence.** Every verdict reports the observations
  behind it and a coarse confidence: an identified AI framework outranks
  utilisation (a training run between batches is not idle), a known game
  executable is weaker evidence because the name table is necessarily incomplete,
  and unattributed GPU load is reported at 0.4 confidence rather than confidently
  mislabelled.

### Safety

- Unattended application is capped at `SettingRisk::Moderate` by a constant, not
  a parameter. `--max-risk dangerous` is **rejected with an explanation rather
  than clamped**, because a caller who asked for it has a different model of what
  the command does and clamping would let them keep it. `Dangerous` covers power,
  thermal, voltage and MSR writes; no unattended loop in ironmon writes one.
- `--apply` without `--confirm` exits 2 and writes nothing.

### Fixed

- **Signal collection resolved the entire ontology to read one number.** Getting
  CPU utilisation via `ontology::resolve::snapshot()` meant enumerating every
  disk's SMART, every PCI device and every USB descriptor on every tuning cycle.
  It now reads the platform CPU stats directly: the module's own test suite went
  from 31.9 s to 3.2 s, and a cycle is 2.55 s.

- **A timing assertion in the new tests was load-dependent.** It bounded absolute
  wall-clock at 5 s, passed when run alone, and failed inside the full suite,
  where hundreds of tests run in parallel and a cycle takes ~2.5 s. The property
  being asserted is that a stop request cuts the sleep short, so it now compares
  against a deliberately long interval instead of a small absolute time.

## [3.7.0] - 2026-08-10

The last of the ontology sweep that can be written without hardware this project
does not have, and one reading withdrawn for being wrong.

### Added

- **11 entities across three clusters**, 123 to 134.

  - **Virtualization** (4): platform, hypervisor, detection method, and whether
    the CPU exposes hardware virtualization. The detection method is recorded
    because virtualization detection is inference, and an agent weighing it should
    see what it rests on.
  - **NUMA** (4): node count, whether access is genuinely non-uniform, and per
    node the processor count and attached memory. A single-node machine reports
    `is_numa: false`, which is a reading; a machine the reader could not inspect
    reports nothing.
  - **ECC** (3): whether error correction is active and reporting, plus
    correctable and uncorrectable counts. Distinct from the per-slot `ecc` entity
    added in 3.4.0, which says the modules *carry* ECC bits: hardware capable of
    correction that is not reporting corrections is indistinguishable from
    hardware doing nothing.

### Fixed

- **`system.virtualization.platform` called a bare-metal desktop a virtual
  machine.** With virtualization-based security enabled — the Windows 11 default —
  the host OS runs as the Hyper-V *root partition*, so CPUID reports
  "Microsoft Hv" on real hardware exactly as it does inside a guest. The
  development machine, an ASUS desktop, was detected as a VM for this reason.

  Telling the two apart needs the partition privilege mask from Hyper-V CPUID leaf
  0x40000003, which IronMonitor does not read. Rather than guess, the entity resolves to
  `unavailable` with that explanation whenever Hyper-V is the detected hypervisor.
  This is the entity an agent consults before trusting every other reading, which
  makes it the worst possible place for a confident guess.

  The underlying `VirtMonitor::is_virtual_machine()` still returns `true` in this
  case for its other callers; that is recorded as open work rather than changed
  here, because its blast radius is wider than the ontology.

- **`system.virtualization.hardware_support` reported `false` on a CPU that
  supports AMD-V.** A running hypervisor masks the virtualization bits from CPUID,
  so `false` under one means "not visible", not "not supported" — the reading told
  an agent this CPU cannot virtualize while it was actively virtualizing. It is
  now `unavailable` with the reason whenever a hypervisor is present.

## [3.6.0] - 2026-08-10

PCIe link state on Windows, which completes the PCI domain.

### Added

- **Negotiated and maximum PCIe link speed and width on Windows**, so
  `pci.{addr}.link.*` resolves rather than being uniformly unavailable. This
  restores the comparison the entity pair exists for: on the development machine
  `0000:01:00.0` reports **x8 against a maximum of x16** — a card in a slot that
  trained at half width, which nothing in the system reports as an error.

  The values come from the PCI driver's device node properties
  (`DEVPKEY_PciDevice_CurrentLinkSpeed` and its three siblings) read through
  `CM_Get_DevNode_PropertyW`. The obvious route, `Get-PnpDeviceProperty`, reads
  exactly the same data and was measured at **25 s across 64 devices** — 0.4 s per
  device, a price a snapshot cannot pay. Two `cfgmgr32` calls per device instead:
  the whole PCI monitor, PowerShell enumeration included, now costs **0.62 s**.

  The property ids were read off a live device rather than recalled — the raw
  `{GUID} pid` is printed alongside the friendly name by `Get-PnpDeviceProperty`,
  and a wrong id returns a real number from the wrong property, which is plausible
  and wrong. The registry route was tried first and rejected: the device property
  store under `Enum\...\Properties` is ACL-blocked for a normal user, and
  unelevated operation is not negotiable here.

  23 of 64 devices report link state; the rest are host bridges and other non-PCIe
  devices, which report none because they have none.

### Fixed

- **A PCIe link speed encoding is a generation, not a rate.** The property holds
  `4` for a Gen 4 link; reporting that as the speed would describe a 16 GT/s link
  as slower than a Gen 1 one. Encodings 1–6 render as their transfer rates, and a
  generation newer than the table is passed through labelled rather than dropped
  or guessed at.

## [3.5.0] - 2026-08-10

Tests generated from the ontology instead of written per feature — and the three
defects they found on their first run.

### Added

- **`tests/ontology_conformance.rs`**, 21 tests that name nothing they check.
  They ask the ontology what exists, resolve it, and assert the rules the ontology
  documents about itself: that every declared entity is reachable, every reading
  traces back to the schema, every absence carries a usable reason, provenance and
  value never contradict, a `nullable: false` entity is never null, a value's JSON
  type matches its unit, nothing violates its declared range, and no enum's
  `Unknown` is dressed as a measurement.

  A domain added next year is covered by all of it on the commit that adds it,
  with no edit to the file. That is the point: the failure mode of a hand-written
  suite is that the newest reader is the least tested one, and this project has
  shipped five `Unknown`-as-measured entities across two releases, each caught by
  eye.

  Includes a coverage report (`-- --nocapture`) of declared, resolved and
  unavailable counts per domain, so the README's claims about reach can be checked
  rather than trusted, and a `the_harness_has_something_to_test` guard — a suite
  that derives its cases from data passes vacuously if the data collapses.

- **`Ontology::template_for`**, mapping a concrete id back to its entity —
  `gpu.0.name` to `gpu.{n}.name`. The inverse of expansion, and what an agent
  holding a reading needs to find its unit, nullability and range. It already
  existed privately in the resolver; the conformance tests need it, and so does
  anyone consuming a snapshot.

- **The PCI domain**, 10 entities, unblocked by the reader fix below.

### Fixed

- **The Windows PCI reader reported a device-instance path as the address and
  never reported the bound driver.** Both now come from
  `HKLM\SYSTEM\CurrentControlSet\Enum`, readable unelevated:
  `LocationInformation` carries the bus/device/function triple in its
  parenthesised tail, and `Service` carries the driver. Addresses are now
  conventional BDF — `0000:7a:00.0`, what `lspci` prints and what the Linux reader
  produces — so ids are stable across reboots. 49 of 64 devices on the development
  machine now report a driver where none did before.

  This is what 3.4.0 withheld the PCI ontology domain over. It ships now.

- **`cpu.cores.physical` was declared and never resolved.** Nothing filled it, so
  it fell through to the unbound sweep as `unavailable` on every machine, despite
  being declared non-nullable. Found by `non_nullable_entities_are_never_null` —
  precisely the class a hand-written suite cannot see, because there is nothing to
  write a test *about*. Now read from the microarchitecture reader.

- **`disk.{n}.nvme.power_state` and `.critical_warnings` were declared
  `identifier`** — a string unit — while resolving to numbers, so a consumer
  trusting the schema would have parsed them wrongly. Both are `count` now.

- **`disk.{n}.capacity` was declared non-nullable** but a USB mass-storage gadget
  with no medium reports no capacity at all. It is nullable, and says why.

## [3.4.0] - 2026-08-10

The ontology sweep continues: memory slot topology and USB inventory, and one
domain deliberately withheld.

### Added

- **19 more ontology entities**, 94 to 113, and an eleventh domain.

  - **Memory** gained 12 for per-slot DIMM topology — locator, populated,
    capacity, rated and configured speed, type, manufacturer, part number, ECC,
    data and total width, voltage. Speeds are counted in MT/s rather than given a
    frequency unit, because megatransfers are not megahertz and conflating them
    halves or doubles every figure. ECC is `derived`, not measured: no SMBIOS
    field states it, and it is inferred from the total width exceeding the data
    width — so it resolves to `unavailable` when either width is unknown, rather
    than to "no ECC" from two zeros being equal.
  - **USB**, a new domain, gained 6 — product, manufacturer, vendor and product
    ids, class and negotiated speed. Ids are keyed on bus and port rather than
    enumeration order, which shifts when an unrelated device is unplugged and
    would silently repoint every id.

  An empty DIMM slot resolves every field to `unavailable` with "this slot is
  empty" rather than to zeros, which would describe a module of no size running
  at no speed.

### Withheld

- **The PCI domain was written, tested, and not shipped.** On Windows the reader
  returns a device-instance path rather than a BDF address, and that path embeds
  a volatile instance id — so `pci.{addr}` would not have been stable across
  reboots, and ontology ids are a contract that may be added to but never
  repurposed. The same reader never populates the driver binding, so the resolver
  asserted "no driver is bound to this device" about every device on a machine
  where they plainly are.

  Fixing the Windows PCI reader is the prerequisite, not the ontology work; the
  declarations and resolver are straightforward once the address and driver
  fields are right. Recorded in HANDOFF.md.

### Fixed

- **The GUI's "USB Devices" heading contradicted the new `usb` domain**, which
  the ontology title-cased to `Usb`. Caught by
  `hardcoded_headings_do_not_contradict_the_ontology` on the commit that added the
  domain — the guardrail working exactly as intended, since a heading that spells
  a domain differently from its id space is what stops a user correlating what
  they see with what an agent can query. `USB` joins `CPU` and `GPU` in the
  acronym table.

- **`usb.{addr}.class` and `.speed` passed an `Unknown` enum variant through as a
  measured identifier** — the fourth and fifth occurrences of that shape in two
  releases, after `disk.{n}.health` and the two TPM fields in 3.3.0. Both now
  resolve to `unavailable` with the reason. The pattern is now called out in
  HANDOFF.md as the thing to check first when adding a resolver.

## [3.3.0] - 2026-08-10

SATA SMART attributes no longer need Administrator — by a different control code
than the one that was planned for it — and the agent-facing ontology grew by half
to name what the disk work had built.

### Added

- **34 new ontology entities**, taking `ironmon describe` from 60 to 94. An agent
  could read a drive's endurance through the library but could not discover it
  through `describe`, `get` or `snapshot`, which is the surface the agentic
  contract is written against.

  - **Disk** gained 15: `health`, `temperature`, `serial`, `kind`, the six
    `smart.*` counters, and six `nvme.*` fields including wear, data units and
    critical warnings. This is the 3.3.0 SMART work made discoverable.
  - **CPU** gained 8 for cache topology — per-level totals and per-instance size,
    line size and sharing map. Sizes are converted to bytes, because the platform
    sources state them in KiB and every other capacity in the ontology is bytes.
  - **Board** gained 11: firmware vendor, product, boot mode and a per-component
    inventory, plus TPM presence, version, manufacturer, status and measured-boot
    state.

  Three of these resolvers initially passed an enum's `Unknown` variant through as
  a measured string — `disk.{n}.health`, `board.tpm.version` and
  `board.tpm.status`. That is the same error as reporting an access denial as
  0 °C: it lets an agent record a health or attestation check that never
  succeeded. All three now resolve to `unavailable` with the reason. `DiskType`
  was also being rendered by `{:?}`, which lowercased `NvmeSsd` to `nvmessd`; the
  mapping is now spelled out.

- **`Unit::Hours`**, so lifetime power-on counts are not misstated in seconds.

- **ATA SMART attributes on Windows, unelevated**, via
  `IOCTL_STORAGE_PREDICT_FAILURE`. The full attribute table, the drive's own
  failure prediction, and — for the first time on Windows at any privilege level —
  reallocated and pending sector counts, which `Get-StorageReliabilityCounter`
  does not expose. Wired into `DiskDevice::smart_info()`, `DiskDevice::health()`
  and `smart::SmartMonitor`, so the collector most callers use no longer needs
  elevation for SATA drives either.

  The plan of record was `IOCTL_ATA_PASS_THROUGH`, and it cannot work: it is
  declared `CTL_CODE(..., FILE_READ_ACCESS | FILE_WRITE_ACCESS)`, so the I/O
  manager rejects it on the zero-access handle before the driver sees it, and a
  read/write handle on `\\.\PhysicalDriveN` requires Administrator. Issued on the
  handle that makes the NVMe path work it returns `ERROR_ACCESS_DENIED`, measured
  on all four drives of the development machine.
  `IOCTL_STORAGE_PREDICT_FAILURE` is `FILE_ANY_ACCESS` and returns the SMART READ
  DATA structure verbatim in its vendor-specific bytes, which is the same data by
  a route the access check permits.

  **The parse has not been run against a SATA drive** — the development machine
  has none, and on its NVMe drives and USB gadget the path returns `NotSupported`,
  which exercises the decline only. The 512-byte structure is parsed in
  `disk::ata_smart`, which is not target-gated, so its 14 tests over synthetic
  buffers run on all three CI platforms. Compare against `smartctl -A` once, on
  real ATA hardware, before trusting a number from it.

### Fixed

- **A drive reporting its own failure could be scored back to healthy.**
  `SmartMonitor::infer_health` recomputed a verdict from counters and overwrote
  whatever was already there. SMART trips on thresholds IronMonitor cannot read, so a
  drive that predicts its own failure can still present clean counters — and did
  present as `Good`. `Failed` is now left alone, being the one verdict that was
  read rather than inferred.

- **The SMART collector no longer re-runs per device.**
  `SmartMonitor::cached_disks()` shares one sweep, process-wide, for
  `CACHE_MAX_AGE` (2 s). `DiskDevice`'s implementations pick a single entry out of
  a list that enumerates every drive, so each of `health()`, `smart_info()` and
  `nvme_info()` used to pay for a full sweep — a subprocess on Windows, one
  `smartctl` per drive on Linux. A four-drive machine making all three calls per
  drive could take twelve sweeps where one would do.

  A sweep measures 1.23 s on the development machine (mean of five, release
  build); a warm `cached_disks()` call is free. The end-to-end saving depends on
  how many drives reach the fallback at all — on a machine whose drives are all
  NVMe or SATA, the passthroughs above already answer without a sweep, and the
  change is not visible above timing noise. It is the machines with USB storage,
  and every Linux machine, that were paying the twelve.

  The window is deliberately far shorter than the polling interval
  `docs/DISK_MONITORING.md` recommends, so a caller polling as advised still gets
  a fresh sweep every time. Callers needing a guaranteed-fresh one construct a
  `SmartMonitor` directly, as before.

### Changed

- The zero-access handle on `\\.\PhysicalDriveN` moved to `disk::windows_device`,
  shared by the NVMe and ATA paths. The access mask is the single detail
  unelevated operation depends on, and the two callers had no reason to state it
  twice.

## [3.2.0] - 2026-08-07

The last three fields that were `None` or a placeholder now carry readings, and
the feature job checks the targets it was missing.

### Added

- **Per-core CPU utilisation and nice time on macOS**, via
  `libc::host_processor_info`. 3.1.0 reported one aggregate parsed from `top` with
  `nice` fixed at 0.0 — the only number in that module that was a convention rather
  than a measurement. Both are now read per core, from cumulative tick counters,
  which is the same thing the Linux reader derives percentages from; so both
  platforms report an average since boot and mean the same by "user".

  3.1.0 declined this on the grounds that unrun Mach FFI is a poor bet. That
  reasoning held for a hand-written structure; it does not hold here. This is a
  `libc` call whose signature the compiler checks, and `tests/macos_readers.rs`
  runs it on `macos-latest` on every push — asserting each core's split accounts
  for that core, and that cores differ from one another, which is what fails if a
  refactor ever fills them from the average. `top` parsing remains as the fallback.

- **Current NVMe power state on Windows**, via Get Features (FID 0x02). The last
  `NvmeInfo` field still fixed at `None`.

  All three drives report state 0, which is both correct for an active drive and
  exactly what an unpopulated field looks like. Querying Temperature Threshold
  (FID 0x04) on the same path returns 0x0165 and 0x0163 — 84 °C and 82 °C, real
  over-temperature thresholds — confirming `FixedProtocolReturnData` is genuinely
  written and the zero is a reading.

### Fixed

- **The Feature combinations job checked default targets only**, so examples and
  test targets went unchecked on every feature set but `--all-features`. It now
  passes `--all-targets`, which immediately surfaced three breakages:

  - `examples/gui.rs` named `ironmonlib::gui` and `eframe` with no
    `required-features`, so `cargo check --all-targets` failed on any set without
    `gui`.
  - A `#[test]` in `platform/windows.rs` called `num_cpus::get()`. `num_cpus` is an
    optional dependency supplied by the `cli` feature, so the library's own test
    target did not build without it. It uses `std::thread::available_parallelism`
    now, which needs no dependency at all.
  - `examples/all_gpus.rs` could not infer its `HashMap` type with no vendor
    feature enabled, because every insertion sits behind a `cfg`.

## [3.1.0] - 2026-08-06

Windows NVMe drives are now read from the controller instead of from WMI, and
macOS reads CPU and memory for the first time.

### Added

- **macOS CPU and memory readers.** `stats::IronMonitor` now reads CPU utilisation,
  memory, swap, uptime and board information on macOS, by parsing `top`, `vm_stat`
  and `sysctl`. Listed as a known gap since 3.0.0 and as a false claim in the
  platform table before that.

  Two things are deliberately not measured:

  - **Per-core utilisation.** `top` reports one aggregate for the package. Cores
    are enumerated with their identity and `None` for utilisation rather than each
    receiving a copy of the average, which would read as measurement.
  - **Nice time.** No macOS command-line tool separates it. `CpuTotal::nice` is not
    an `Option`, so it is reported as 0.0 — the one number in that module which is
    a convention rather than a reading, and it is marked as such in the source.

  Both would come from `host_processor_info`. That is Mach FFI, and this code was
  written by cross-compilation from a Windows machine — an unrun `unsafe` block
  reading a structure at the wrong offset produces plausible wrong numbers, which
  is the failure this project spent 3.0.0 removing. Parsing documented textual
  output can be tested; `tests/macos_readers.rs` runs on `macos-latest` on every
  push and asserts what a reading has to satisfy to be one.

- `IronMonitor::cpu()`, `IronMonitor::memory()` and `IronMonitor::uptime()`, which read one thing
  each. `IronMonitor::snapshot()` requires every reader to succeed, so on macOS — where
  GPU, power and temperature are still unimplemented — it fails and takes the
  working readers down with it. These accessors expose what works without the rest
  having to pretend.

- `platform::macos`, parsers for `top`, `vm_stat` and `vm.swapusage` with 12 unit
  tests over captured output. Not target-gated, so they are tested on all three
  platforms rather than only on the one nobody here can run.

- **NVMe passthrough on Windows** via `DeviceIoControl` with
  `StorageDeviceProtocolSpecificProperty`, issuing Identify Controller and the
  SMART/Health log page. Fields that were `None` in 3.0.0 now carry readings:
  temperature, power-on hours, power cycles, media errors, wear, controller id,
  namespace count, NVMe version, total and unallocated capacity, data units read
  and written, host read and write commands, critical warnings, and the available
  power state table.

  Measured on this machine, unelevated:

  ```
  PhysicalDrive0  Samsung SSD 9100 PRO 4TB     52.9 °C  27 h  NVMe 2.0.0  cntlid 1
  PhysicalDrive1  Samsung SSD 990 PRO 4TB      45.9 °C  40 h  NVMe 2.0.0  cntlid 1
  PhysicalDrive2  Samsung SSD 970 EVO Plus 2TB 45.9 °C  58 h  NVMe 1.3.0  cntlid 6
  PhysicalDrive3  USB enclosure                NotSupported
  ```

- `disk::nvme_log`, parsers for the health log page and Identify Controller with
  13 unit tests over synthetic structures. They are not target-gated: the byte
  layouts are the same everywhere, so gating them to Windows would mean their tests
  ran on one of the three platforms CI covers.

### Changed

- **The passthrough needs no elevation, and the documentation saying otherwise was
  wrong.** `\\.\PhysicalDriveN` has to be opened with a desired access of *zero*;
  requesting `GENERIC_READ | GENERIC_WRITE` is what demands Administrator. 3.0.0
  scoped this work as "elevated Windows passthrough" and left the fields `None` on
  that assumption. Only the `Get-StorageReliabilityCounter` path ever needed
  elevation, and that is now the fallback for SATA and USB rather than the primary
  route for everything.
- `nvme_info()` decides whether a device is NVMe by whether the controller accepts
  the NVMe protocol, not by comparing WMI's `MediaType` string. That comparison is
  what made 3.0.0 refuse every NVMe drive in this machine before `BusType` was
  added; the device's own answer cannot drift out of sync the same way.
- Windows NVMe health is graded from the controller's critical warning bits, wear
  and remaining spare. Reliability-degraded and read-only bits are `Critical`;
  other warnings, wear at 100%, or spare below the drive's own threshold are
  `Warning`.
- `smart_info()` reports `reallocated_sectors` and `pending_sectors` as `None` on
  NVMe rather than 0. They are ATA concepts; zero would assert a count that was
  never measured.

### Fixed

- The reply payload is located from the returned descriptor rather than from the
  offset the request used. Those differ by four bytes, and reading the wrong one
  produces shifted-but-plausible data rather than an error — model strings arrived
  as `"ung SSD 9100 PRO 4TB"`, temperatures as 3 K, power-on hours as
  2139160387885137115025686659072.

## [3.0.1] - 2026-08-06

### Fixed

- **The `cli` feature did not build on its own.** `ironmon` called
  `ironmonlib::gui::run()` in the `None` arm of the command match without a
  `cfg(feature = "gui")`, so `--no-default-features --features cli` failed with five
  unresolved imports, and the arm also duplicated the `not(feature = "gui")`
  fallback that already existed twenty lines below. README has advertised `cli` as a
  standalone flag since the first release. Built without `gui`, `ironmon` with no
  subcommand now launches the TUI, which is what that fallback was written to do.

  Every CI job was green throughout, because every job enabled `gui` as well.
  `--all-features` is the one combination a user is least likely to choose, and it
  cannot catch a feature that only compiles because another supplies what it is
  missing.

- **`cargo install iron-monitor` failed on Linux.** `handle_jetson_command`
  dispatched to `handle_jetson_clocks`, `handle_nvpmodel` and `handle_swap` under
  `cfg(target_os = "linux")` alone, while all three functions are additionally gated
  on `jetson-utils`. `full` — the default feature set — includes `cli` and
  deliberately omits `jetson-utils`, so the default Linux build failed with three
  E0425s. Every published version carried it. On Linux without `jetson-utils`,
  `ironmon jetson …` now exits with a message naming the feature to rebuild with,
  matching how the non-Linux case already behaved.

  This one is why the new CI job checks combinations rather than the union:
  `--all-features` enables `jetson-utils` and never saw it, and it cannot be
  reproduced from Windows — `cli` pulls `openssl-sys`, whose build script needs
  Linux headers, so the cross-target check the project relies on stops short of it.

### Added

- CI job **Feature combinations**, checking each advertised feature in isolation on
  every push. It reads the feature list from the manifest, so a feature added later
  is covered without editing the workflow. It found the Linux `full` breakage above
  on its first run.

### Documentation

- Installation covers crates.io: `cargo install iron-monitor`, the Linux system
  packages the default GUI build needs, and the `default-features = false`
  dependency line library consumers want.
- Quick Start has a worked SMART/NVMe example — 3.0.0's headline feature shipped
  without one. It shows what the `Option` counters require of callers and states the
  Windows elevation behaviour.
- The feature flag list was missing `apple`, `cpu`, `npu`, `io`, `network` and
  `gui`.
- `ironmon` with no subcommand was documented as launching the TUI. It launches the
  GUI in a default build.

## [3.0.0] - 2026-08-06

SMART and NVMe support, and the removal of a fabrication the work uncovered.

### Added

- **`DiskDevice::smart_info()`** on Linux and Windows. Returns the drive's health
  verdict and whatever attributes the platform exposes. Verified on Windows against
  four physical drives.
- **`DiskDevice::nvme_info()`** on Linux and Windows. Identity — model, serial,
  firmware, capacity — needs no privileges. Linux additionally reads the controller
  id and namespace count from `/sys/class/nvme`, which Windows does not expose
  unelevated. Returns `NotSupported` for non-NVMe devices.
- `DiskDevice::health()` is now derived from SMART on both platforms. The Linux
  implementation previously returned `Healthy` whenever the device file existed,
  reporting "the kernel enumerated this drive" as a clean bill of health.

### Fixed

- **SMART reported fabricated readings for every drive.**
  `Get-StorageReliabilityCounter` requires elevation; unelevated it fails with
  `PermissionDenied`. That failure was swallowed by `-ErrorAction SilentlyContinue`
  and replaced with `0`, so IronMonitor reported every drive as healthy with a
  temperature of 0 °C and zero power-on hours. An access error was being presented
  as a measurement.
- **Every drive was named `PhysicalDrive0`.** `DeviceId` arrives from PowerShell as
  a JSON *string* (`"1"`), and `as_u64()` returned `None` for all of them, so the
  `.unwrap_or(0)` fallback collapsed four distinct drives into one name.
- **Health was graded from an empty scorecard.** With counters defaulting to zero,
  a drive whose data could not be read took no penalties, scored 100, and was
  declared `Good` with "100% life remaining" — a confident verdict derived from
  nothing. Scoring now requires evidence; with none, the platform's own verdict
  stands and no life estimate is published.
- **Every NVMe drive was classified as a plain SSD.** Windows answers `MediaType`
  (the medium, `"SSD"`) and `BusType` (the transport, `"NVMe"`) separately, and only
  `MediaType` was read — which then made `nvme_info()` refuse to answer for the
  drives it was written for.

### Changed — breaking

- `smart::SmartDiskInfo`'s counters are now `Option`: `temperature_celsius`,
  `power_on_hours`, `power_cycle_count`, `reallocated_sectors`, `pending_sectors`,
  `uncorrectable_errors`, `total_bytes_written`, `total_bytes_read`. `None` means
  the platform would not report it; it does not mean zero.
- `disk::NvmeInfo`'s `nvme_version`, `unallocated_capacity`, `controller_id`,
  `num_namespaces`, `power_state` and `critical_warnings` are now `Option` for the
  same reason. A `controller_id` of 0 is a real controller and a `num_namespaces`
  of 0 is a real answer; neither can stand in for "not read".
- `SmartMonitor::max_temperature()` returns `Option<u32>` rather than `0` when no
  drive reported a temperature.

## [2.1.5] - 2026-08-06

### Fixed

- **`docs/CPU_MONITORING.md` described the Windows implementation as a skeleton.**
  It called `src/silicon/windows.rs` "basic structure and skeleton" with a
  "placeholder for WMI and Performance Counter integration", and marked every
  Windows cell 🚧. The file is 644 lines across 25 functions with no TODO markers,
  and `ironmon cli cpu` returns twenty-four distinct per-core figures on a Ryzen 9
  9900X — not one average replicated, which is a defect this repository has shipped
  before. Per-core utilization corrected to ✅ on that evidence; temperature
  corrected to ❌, since `ironmon cli temperature` finds no sensors and the board
  ones need a signed kernel driver IronMonitor does not ship.
- **The same table marked macOS "✅ Complete" for a column nothing has ever
  exercised.** The crate could not build on macOS until 2.1.2. Those marks are now
  labelled unverified rather than corrected, because verifying them needs a Mac.
- The table also did not say which subsystem it describes. `src/silicon/` is the
  enhanced per-core layer; `CpuStats` / `stats::IronMonitor` is the general path and has
  no macOS implementation at all. A ✅ in one table did not imply the other worked,
  and nothing said so.

### Verified, not changed

- Every documented flag (`--format`, `--watch`, `--frame`, `--script`,
  `--writable`, `--search`) exists on the command it is shown with.
- Every documented entity id resolves: `gpu.0.power.limit` and
  `gpu.0.thermal.temperature` return measured values, and the template form
  `gpu.{n}.name` is rejected with the guidance the contract promises.
- `docs/DISK_MONITORING.md`'s "trait defined, not yet implemented" claims for SMART
  and NVMe are accurate — both are trait defaults returning `NotSupported` with no
  platform override.

## [2.1.4] - 2026-08-06

### Fixed

- **The documented commands did not exist.** The README's main usage block showed
  `ironmon cpu`, `ironmon gpu`, `ironmon memory`, `ironmon processes`, `ironmon audio`,
  `ironmon bluetooth`, `ironmon displays` and `ironmon usb`. All of these live under
  `ironmon cli`, and `displays` is `display`. Anyone following the quick-start hit
  `error: unrecognized subcommand` on their first command — while the watch-mode
  examples ten lines below used the correct form. `CLI.md` had twenty-nine such
  lines and `docs/UTILITIES.md` one more.

### Added

- A test comparing every `ironmon …` invocation in the documentation against
  `ironmon describe --commands`, the machine-readable catalog the binary generates
  from its own argument parser. Documentation is not compiled, but the binary can
  be asked what it accepts, so the two can be checked against each other. It found
  `CLI.md`, which a manual sweep of the same question had missed entirely.

## [2.1.3] - 2026-08-05

### Fixed

- **The README's platform table claimed macOS CPU and memory support that does not
  exist.** Both were marked fully supported. Neither has a reader — the claim
  survived because the crate could not build on macOS at all, so nothing ever
  contradicted it. Corrected, with the gap and the reason stated beneath the table,
  and macOS disk's I/O-counter limitation noted alongside.
- **The Contributing section listed the GUI as "planned but not yet
  implemented"** while three other sections of the same README documented how to
  run it.
- The 2.1.2 changelog entry described only the manifest bug and the CI gaps, having
  been written before the 114 compile errors, the lint pass, and the defects that
  surfaced behind them. It now records what actually shipped.

This is a documentation-only release. 2.1.2's published page carries the incorrect
table, which is the reason for shipping a patch rather than waiting.

## [2.1.2] - 2026-08-05

### Fixed

- **The crate did not build on Linux or macOS — and never had.** A missing
  `[dependencies]` header meant the CLI, TUI, GUI, remote-AI-backend, and logging
  dependencies, plus the unconditional `chrono` and `async-trait`, were all parsed
  as a continuation of `[target.'cfg(windows)'.dependencies]`. On any other
  platform the `cli`, `gui`, and `remote-backends` features enabled code whose
  crates were absent from the dependency graph, and the build failed outright with
  `cannot find crate 'crossterm'` and a dozen more like it.

  This dates to the **initial commit** (2026-01-21) and shipped in **every one of
  the eight versions published to crates.io**, while the README advertised a native
  desktop application for Windows, Linux, and macOS. It survived because
  development happened on Windows, where the manifest is correct, and because a
  section header is invisible when you are reading the dependency beneath it —
  every individual line was right.

  `cargo tree --target x86_64-unknown-linux-gnu --features full` shows it in one
  command, and now a test asks the manifest the same question on every run.

- **The GUI could not build on Linux either.** `eframe` is declared with
  `default-features = false`, which drops its windowing backends along with the
  renderer defaults. Windows and macOS each have one backend and winit finds it
  without a feature; Linux has two, winit selects neither, and the build stops at
  `compile_error!`. `x11` and `wayland` are both named now.
- **`nvidia` did not resolve on macOS.** `nvml-wrapper` was declared only in the
  Linux and Windows target sections, but a feature is not target-conditional, so
  enabling it elsewhere referenced a crate absent from the graph. It is
  cross-platform now — nvml-wrapper is a runtime loader for libnvidia-ml, so it
  compiles anywhere and finds no library where there is no driver.

- **114 compile errors in code that had never been built** — 73 on Linux, 41 on
  macOS, across roughly forty files. The manifest bug meant cargo never reached
  ironmon's own source on those platforms, so none of it had ever been checked. Most
  were mechanical (a `IronError::IoError` variant that does not exist, at 23
  sites; structs and enums whose fields and variants had been renamed under code
  that never recompiled). Several were not:

  - `motherboard/linux.rs` tested `contains("DP")` before `contains("eDP")`. Since
    `"eDP-1"` contains `"DP"`, **every internal laptop panel was reported as an
    external DisplayPort** and the eDP arm was unreachable.
  - `disk/linux.rs` indexed `parts[1]` of a `dev` file split on `':'` — a panic on
    any malformed file, sitting behind a `.unwrap_or(0)` that made it look handled.
    The major/minor pair it parsed was never read by anything.
  - `disk/macos.rs` and `gpu/intel.rs` reported measurements they never made: one
    combined `iostat` throughput figure halved into read and write, and a
    `gpu_memory: 0` meaning "shares system memory". Zero is not unknown.
  - `services.rs` borrowed `self` mutably and immutably at once; `numa/mod.rs`
    referenced a binding one line after it left scope; `process_monitor.rs` divided
    `f64` by `u64`; `hwmon/smart.rs` called `glob::glob` with no `glob` dependency.

- **`thermal.<none>` contradicted the readings beside it.** The resolver always
  enumerates cpu, gpu and motherboard and writes an `unavailable` row for each, then
  *also* emitted a summary row saying the domain enumerated nothing. Only reachable
  where no sensor reads at all, which is why the first macOS run found it and no
  Windows run could. The read-failure path still emits it, correctly — that path
  returns before pushing anything.

- **The plausibility suite encoded Windows' CPU accounting model.** It demanded
  user+system+idle sum to ~100. Linux divides by every field in `/proc/stat`, so
  nice, iowait, irq, softirq and steal are real time belonging to none of the
  three; a virtualized runner sits at 94%. The invariant that holds everywhere is
  that three shares of one total cannot exceed it.

- **The process-state set omitted `'I'`** — idle kernel thread, reported by Linux
  since 3.13 and carried by every `kworker/R-*`. The reader also fell back to `'?'`
  for an unreadable state where the rest of IronMonitor uses `'U'`.

- **102 clippy findings in never-linted Linux code**, which CI rejects under
  `-D warnings`.

### Added

- `tests/manifest_portability.rs`: every crate reachable from a feature must be
  declared in the plain `[dependencies]` table unless it is one of the three
  (`drm`, `drm-ffi`, `plist`) that are target-gated by design with call sites
  gated to match; and crates used without a `cfg` guard must not be target-gated.
  Verified against the pre-fix manifest, where it names each affected crate.

### Known gaps

- **IronMonitor does not measure CPU or memory on macOS.** There are readers for Linux
  and Windows and none for macOS: `CpuStats::new` and `MemoryStats::new` return
  empty, and `stats::IronMonitor`'s ten platform functions report `UnsupportedPlatform`
  naming `IronMonitor`, which does have working macOS paths. The crate builds,
  lints, and passes its suite there — it does not yet read hardware. Tests that
  assert something *about a reading* are gated on `platform_has_hardware_readers()`
  so the gap is named once rather than hidden behind scattered `cfg`s. Implementing
  this means sysctl and IOKit work that has to be verified on real hardware.

### Fixed — CI

The manifest bug above was invisible to development but not to CI, which had been
red on every push. These are the reasons it stayed red without anyone learning
anything from it.

- **CI never compiled the `jetson-utils` feature.** Every job ran
  `--features full`, and `full` deliberately omits it — so
  `src/utils/{swap,clocks,power_mode,security}.rs` was never checked, linted, or
  tested by any CI run. That is shipped code a user can enable, and it is the most
  safety-sensitive in the repository; the feature's own comment points at
  SECURITY.md. It had accumulated 25 clippy warnings, invisible. `jetson-utils`
  pulls in no dependencies — it only gates `cfg` — so building everything costs
  nothing. All jobs now use `--all-features`.
- **Clippy ran without `--all-targets`,** so examples and integration tests were
  never linted. That is how `examples/agent_backends.rs` came to abort on its own
  second step, unnoticed, for months.
- **Three doc tests never ran in CI:** `--features full` runs 70, `--all-features`
  runs 73, and the three it skipped did not compile.
- The documentation tests added in 2.1.1 panicked outside a git checkout, which
  would have broken `cargo test` for anyone building from the packaged tarball or a
  vendored copy. They now skip when there is no checkout to enumerate, while still
  refusing to pass vacuously inside one.

## [2.1.1] - 2026-08-05

### Fixed

- **The README rendered six broken images**, on GitHub and on the crates.io page
  for 2.1.0. Two "Screenshots" sections linked PNGs under `docs/images/` that were
  never added — only the directory's own README describing how to capture them was
  ever committed. A broken image fails silently: the markdown is valid and the
  build is green, so nothing caught it. The TUI section now embeds a real frame
  captured through the same headless renderer the tests drive, so it cannot drift
  from what the TUI actually draws; the GUI section says plainly that screenshots
  are outstanding and how to see the interface meanwhile.

### Added

- A test asserting every relative markdown link and image resolves, and a second
  asserting documentation carries no machine identifiers. The embedded TUI frame is
  real tool output, which includes the host's name — sanitizing it was a manual
  step, and manual steps are the ones skipped next time the capture is refreshed.

## [2.1.0] - 2026-08-04

Model names in this repository were frozen lists that had aged out of date. The
fix is not new names — those would age too — but reaching for the provider's own
listing wherever one exists, and saying plainly that anything still hardcoded is a
guess.

### Fixed

- **The GUI's model dropdown never actually asked hosted providers what they
  serve.** `fetch_models_async` read the API key only from the tab's text field.
  Both hosted listing endpoints require authentication, so a user with
  `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` already exported — the normal case — sent
  an unauthenticated request, got a 401, received an empty list, and silently fell
  back to hardcoded names. The live-refresh path existed but could not be reached.
  The key is now resolved from the field *or* the environment, and setting a key in
  the tab drops the cached unauthenticated result so the provider is asked again.

### Changed

- **`ironmon ai manifest` no longer advertises a fixed `supported_models` list.**
  Seven export formats each carried a frozen array of model ids. Nothing consumed
  them, every one had gone stale (the OpenAI entry still listed `gpt-4o`, the
  Anthropic entry `claude-3-opus-20240229`), and the claim was wrong in principle:
  the manifest describes *tools*, so any model that calls tools in that provider's
  format can use it. Replaced with `model_discovery`, naming the provider's listing
  endpoint — which answers the question for the caller's own account and cannot
  rot. **Consumers reading `supported_models` must switch to `model_discovery`.**
- **Refreshed the model names that remain**, in the GUI's fallback list, the
  auto-detect defaults (now the named constants `DEFAULT_OPENAI_MODEL`,
  `DEFAULT_ANTHROPIC_MODEL`, `DEFAULT_GITHUB_MODEL`), the examples, and the docs.
  The GUI labels these "Not listed by provider" because that is what they are.
- **GitHub Models is no longer offered — GitHub retired it on 2026-07-30.** The
  playground, catalogue, inference API, and BYOK access are all shut down. Backend
  discovery previously treated the mere presence of `GITHUB_TOKEN` as an available
  backend; that variable is set on a great many machines for unrelated reasons (the
  `gh` CLI, CI), so those users were handed a provider guaranteed to fail, presented
  as ready to use. Discovery no longer offers it, the GUI labels it retired, and
  `BackendType::RemoteGitHub` and `DEFAULT_GITHUB_MODEL` are `#[deprecated]`. Both
  are retained so saved configurations still deserialize rather than failing to
  load.
- **Backend capability metadata was model-pinned and wrong.** `RemoteAnthropic`
  reported a 200K context and `$3.00/1M` ("Claude 3.5 Sonnet"); `RemoteOpenAI`
  reported 128K and `$5.00` ("GPT-4o pricing"). Context is now 1M for both, and the
  per-token cost is `None` — a single figure cannot express a hosted rate, which
  splits into input and output prices and varies per model. `None` is documented as
  "not known", not "free", and the discovery example now distinguishes the two
  rather than printing "Free" for a paid API.

- **The `agent_backends` example ran only two of its seven sections.** It opened by
  building `AgentConfig::new(ModelSize::Medium)` and calling it "the rule-based
  built-in backend, always available". No such backend exists — `Agent::new`
  requires one to be configured — so the example aborted on its own second step and
  everything after it was unreachable. It now uses the discovered backend, and
  reports a missing or failing backend instead of dying on it.

### Added

- Tests pinning the invariants behind all of the above: a provider shipping
  hardcoded names must have somewhere to find a credential (or those names can
  never be replaced), a local provider must never guess a model name, and no export
  format may ship a frozen model catalogue again.

## [2.0.1] - 2026-08-04

The first 2.x published to crates.io. 2.0.0 was tagged but never published: these
two fixes landed after the tag, and shipping them as 2.0.0 would have made the tag
point at code that was not what was released.

### Fixed

- **`--offline` announced a guarantee it did not enforce.** The flag is documented
  as disabling all network features; it set two environment variables and stopped.
  `consent::is_offline_mode` had no callers anywhere. A hosted backend received
  this machine's hardware inventory — including GPU UUIDs and PCI bus ids, which
  are stable device identifiers — while the tool reported that it would not.
  `Agent::ask` now refuses any backend whose `runs_on_host` is false when offline
  mode is set. The predicate is egress rather than "makes a network call", so a
  model served over loopback stays usable.
- **Two README links did not resolve.** `docs/AI_AGENT.md` was dropped by an
  earlier correction whose check only looked at the repository root, so any target
  in a subdirectory read as missing. The link checker now resolves each target
  relative to the file containing it and walks every tracked markdown file; three
  further broken links in CLI.md were found and fixed the same way.

## [2.0.0] - 2026-08-04

Tagged, not published. See 2.0.1.

A major version because five public signatures changed, not because the release is
larger than most. Under SemVer that is a major bump regardless of how unlikely the
breakage is, and this project states it adheres to SemVer — calling it 1.6.0 would
be a claim contradicted by the artifact.

### Breaking

`GpuCollection` no longer hands out references to the `Box` holding a GPU. Five
methods return the trait object directly:

| Before | After |
|---|---|
| `get(i) -> Option<&Box<dyn Gpu>>` | `get(i) -> Option<&dyn Gpu>` |
| `get_mut(i) -> Option<&mut Box<dyn Gpu>>` | `get_mut(i) -> Option<&mut (dyn Gpu + 'static)>` |
| `nvidia_gpus() -> Vec<&Box<dyn Gpu>>` | `nvidia_gpus() -> Vec<&dyn Gpu>` |
| `amd_gpus() -> Vec<&Box<dyn Gpu>>` | `amd_gpus() -> Vec<&dyn Gpu>` |
| `intel_gpus() -> Vec<&Box<dyn Gpu>>` | `intel_gpus() -> Vec<&dyn Gpu>` |

Callers that stored the result as `&Box<dyn Gpu>` need the type updated; call sites
that immediately invoke a trait method need no change, since `&Box<T>` already
auto-dereferenced. Migration is `.as_ref()` where a `Box` is genuinely wanted.

### Added — Machine-readable ontology over every reading

Every value IronMonitor reports now has a stable dotted id, a unit, and a **provenance**
saying where it came from. The three interfaces share one vocabulary rather than
three.

- `ironmon describe` — the schema: ids, units, provenance, descriptions. Touches no
  hardware, so it is identical on every machine and can be fetched and cached
  ahead of time.
- `ironmon describe --commands` — the command surface, walked out of the argument
  parser so it cannot drift from what the binary accepts.
- `ironmon describe --writable` — settings backed by a registered apply handler.
  Generated from the handler registry, so the schema cannot advertise a write the
  binary will reject.
- `ironmon get <id>` — one reading. Exits 1 for an unknown id, 2 for a known id
  with no value here, so a caller can tell "no such thing" from "nothing to
  report".
- `ironmon snapshot [--validate]` — every resolvable entity, across all ten domains.
  The count varies with the hardware present and the process table, so it is not
  quoted here.

`provenance` is the point: `measured` was sampled now, `specification` is a
published constant true of the hardware but not observed here, `derived` names its
inputs, and `unavailable` carries the reason it could not be read. Only `measured`
may be treated as a live observation. Nothing substitutes zero or a plausible
constant for a value it could not obtain, and a reading outside its declared range
is withheld rather than clamped.

### Added — Headless inspection of the TUI and GUI

Both interactive surfaces can now be read and driven without a terminal or a
display.

- `ironmon tui --frame [--tab NAME]` and `ironmon gui --frame [--tab NAME]`
- `ironmon tui --script` — `goto`, `key`, `refresh`, `capture`, `assert`, `refute`.
  Key steps go through the same handler the interactive loop calls.
- `ironmon gui --script` — the same minus `key`, since GUI tabs are addressable by
  name.

### Fixed

- The Profiles tab appeared dead. It was rendering all 19 groups; `RichText::strong()`
  resolved to the panel colour, so every `strong()` label in the GUI was invisible. The
  light theme had the same bug in white. The AI tab's reported failure has the same
  cause.
- Collapsing-header triangles (U+25BE/U+25B8) rendered as tofu — Geometric Shapes
  are not covered by the bundled emoji font — and duplicated the arrow the widget
  already draws.
- Numerous readings were constants presented as measurements: a boot time of exactly
  45 seconds whenever the real one could not be read, `min_freq = max_freq / 4`,
  Secure Boot inferred from a registry key's existence rather than its value, Apple
  GPU clock and power ceilings from a core-count table, macOS disk throughput halved
  into a fabricated read/write split, and NIC link speed divided by 1000 for any
  unrecognised unit.
- The port scanner reported `Filtered` — a specific claim that a firewall dropped
  the packet — for host-unreachable and permission-denied errors that never reached
  the host.

### Changed

- `lto = "thin"` in the release profile. Fat LTO triggers a compiler ICE on rustc
  1.97.1 for this crate, in lint-level sorting under `-C lto -C codegen-units=1`.
  Binary grows 3.6%. Restore `lto = true` once the toolchain is fixed.
- `ai_api::HardwareOntology` is superseded by `crate::ontology`. It carries no
  provenance, cannot resolve an id to a value, and no command exposes it.

## [1.5.0] - 2026-07-29

### Added — Lock-free snapshot pipeline

Hardware collection moved off the render thread entirely. A dedicated collector
thread owns every hardware handle and publishes immutable snapshots into an
`ArcSwap` slot; UI threads do a lock-free atomic load and never touch a driver.

- Independent collectors run **concurrently** within a tick, so tick cost is the
  slowest single collector rather than the sum of all of them.
- A **warm-up snapshot** is published from the collectors needing no driver setup
  (CPU, memory, system stats, disks), so first data no longer waits on GPU
  enumeration.
- `Snapshot` carries per-collector timings, making collection cost observable
  rather than guessed at.

Measured on a 3-GPU Windows host:

| | Before | After |
|---|---|---|
| Collection tick | 1184 ms serial | 338 ms concurrent |
| Process collector | 685 ms | 186 ms |
| GPU collector | 775 ms | 337 ms |
| Cold start (first data) | 8–12 s | ~192 ms |
| Frame data access | blocking driver call | 19 ns |

TUI, GUI and `ironmon record` all read from the pipeline. TUI and GUI now repaint only
when a new snapshot arrives instead of on a fixed tick.

### Added — IronWorks as the built-in inference engine

[IronWorks](https://github.com/nervosys/ironworks) is now the default backend and the
only engine ironmon ships against, reached over its OpenAI-compatible server. Every
other backend is an external provider (`BackendType::is_builtin_engine`).

### Added — CLI AI providers

`ollama`, `claude`, `codex` and `gemini` can be driven as subprocesses, detected on
`PATH`. No API key needed, since the tool is already authenticated.

`BackendType::runs_on_host` distinguishes local *inference* from a local *process*:
only `ollama` runs the model on your machine; the others relay prompts to their
vendor. Backend selection prefers on-host inference.

### Fixed — data that was fabricated or silently incomplete

- **Windows per-core CPU was the system average replicated.** `read_cpu_stats` used
  `GetSystemTimes` (system-wide) and assigned the identical value to every core, so
  a 24-core machine drew 24 identical bars that looked measured. Now uses real
  per-processor data via `NtQuerySystemInformation`.
- **Unelevated process listings omitted every SYSTEM process.** Processes whose
  `OpenProcess` failed were dropped entirely, and the plausible count hid it. They
  now emit a reduced row from the Toolhelp snapshot (315 → 439 processes observed).
- **The TUI invented memory readings** — a hardcoded 32 GB/16 GB fallback rendered
  identically to measured values.
- **Windows fan RPM was hardcoded to 1000** whenever `ActiveCooling` was true.
- **The GUI network chart plotted `(cumulative_bytes / 1MB) % 10000`** — a sawtooth
  of a running total, labelled as throughput. Now plots actual MB/s.
- **`ironmon record` sampled at the wrong rate.** It slept the full interval on top of
  ~1.2 s collection, so `--interval 1` recorded every ~2.2 s and every derived rate
  was wrong by that ratio.
- **macOS audio never classified any device as Input** (`has_output || true` made the
  flag dead).
- **vLLM requests all 404'd** — its default endpoint lacked `/v1` while discovery
  probed a different path, so it reported available but never worked.
- **GPU snapshots short-circuited on the first error**, blanking all devices when one
  failed. Now index-preserving, so a failing device keeps its slot.
- **llama.cpp was never discovered** despite being implemented; availability was
  hardcoded to `false`.

### Fixed — documentation that contradicted the code

- All 53 doc tests were failing (`use ironmon::` vs the actual crate name `ironmonlib`),
  plus six examples that had drifted from their APIs. All 70 now pass.
- The agent module documented four "Design Principles" that were each false: it
  claimed to run in a separate thread (no thread is ever spawned), to offer
  100M/500M/1B models (none are loaded), to keep all processing local (a backend is
  required, and hosted ones transmit telemetry), and to be consent-aware (no code
  path touches the consent module).
- `ask_with_timeout` ignores its timeout argument; this is now documented rather
  than implied otherwise.
- The README and `docs/AI_AGENT.md` described a rule-based offline fallback engine
  that does not exist, and linked to a missing `LOCAL_AI_BACKENDS.md`.

### Changed

- `GpuCollection::snapshot_all` queries devices concurrently; added
  `snapshot_all_partial` for index-preserving partial results.
- New `platform::windows::logical_drives` replaces a 26-letter `fs::metadata` probe
  that blocked on disconnected network drives.
- Process enumeration memoizes per-PID usernames on `(pid, creation_time)` and
  requests only `PROCESS_QUERY_LIMITED_INFORMATION`.
- Repository formatted with `cargo fmt --all`; `cargo fmt --check` had been failing.

## [1.4.0] - 2026-05-14

### Added — Hardware Profile Inspector (NVPI / XTU / Ryzen Master / nvme-cli)

A unified read-write inspector for vendor driver settings, application profiles,
and tunable hardware parameters across five subsystems (GPU, CPU, NVMe, Display,
Memory). Inspired by NVIDIA Profile Inspector, Intel XTU, AMD Ryzen Master, and
`nvme-cli`.

- **Five subsystem providers** with platform-specific surfaces:
  - **GPU**: NVML driver state (clocks, power limit, ECC, persistence), Linux
    `/sys/module/nvidia/parameters`, AMD `/sys/class/drm/card*/device/` knobs
    (DPM state, perf level, OD tables, hwmon power cap), Intel `gt_*_freq_mhz`,
    Windows display-class registry walk (~390 vendor driver overrides), full
    NVIDIA DRS binary database scan (~12k per-application profiles from
    `nvdrsdb*.bin`).
  - **CPU**: Linux cpufreq policy + governor + EPP, `intel_pstate` toggles, MSR
    `0x610` PL1/PL2 decode, Windows active power scheme + scheme enumeration via
    `PowerEnumerate`, RAPL energy domains.
  - **NVMe**: Linux `/sys/class/nvme/*` controller + namespace queue policy,
    typed Get-Features admin command (`NVME_IOCTL_ADMIN_CMD`) decoding Power
    Management, Volatile Write Cache, Number of Queues, Async Event Config,
    APST, Software Progress Marker.
  - **Display**: existing `DisplayMonitor` per-display state plus full EDID
    block decoder (manufacturer PNP code, product/serial, monitor name,
    preferred timing, DPMS flags, gamma).
  - **Memory**: SMBIOS Type 17 per-DIMM (rated vs configured speed flags
    XMP/EXPO-off conditions), plus XMP 2.0/3.0 + AMD EXPO SPD blob decoder on
    Linux (`/sys/bus/i2c/devices/*/eeprom`).
- **Apply (write) layer** with audit log:
  - `ApplyHandler` trait, `ApplyOutcome { status, message, timestamp, ... }`
  - JSON-lines audit log at `%LOCALAPPDATA%\ironmon\profile_audit.log` (Windows) /
    `$XDG_STATE_HOME/ironmon/profile_audit.log` (Linux), every attempt logged
    including refused/not-writable
  - Confirmation required (`--confirm` CLI flag, or `confirm=true` + env
    `IRONMON_ALLOW_AGENT_WRITES=1` double-gate for MCP)
  - Concrete handlers: NVIDIA persistence mode (Linux NVML), Linux cpufreq
    governor, Linux AMD `power_dpm_force_performance_level`, Linux Intel
    `gt_max_freq_mhz`, Windows `PowerSetActiveScheme`
- **Analysis surface**:
  - `ironmon profile diff <baseline.json>` — drift report between snapshots
  - `ironmon profile deviations` — settings changed from declared default,
    sorted by risk
  - `ironmon profile explain <id>` — full setting metadata with related-setting
    lookup
  - `ironmon profile active [--matched]` — running PIDs joined against NVIDIA
    DRS database
  - `ironmon profile search <query>` — substring match across all settings
  - `ironmon profile watch [--interval]` — continuous change detection
  - `ironmon profile bench` — per-provider snapshot timing
  - `ironmon profile schemes` — enumerate Windows power schemes with friendly
    names
- **Caching**: `CachedProfileInspector` with per-subsystem TTLs (NVMe 30s, GPU
  15s, Memory 60s, Display 5s, CPU 2s); process-global hit/miss counters.
- **Interfaces**:
  - 16 CLI subcommands under `ironmon profile`
  - 12 MCP tools (`list_profile_subsystems`, `get_profile_settings`,
    `search_profile_settings`, `get_active_app_profiles`, `get_profile_deviations`,
    `explain_profile_setting`, `list_writable_profile_settings`,
    `apply_profile_setting`, `benchmark_profile_providers`, plus existing ones)
  - GUI: dedicated "🛠 Profiles" tab with deviation panel, audit log tail,
    risk-color-coded settings tree, text filter, subsystem chips, cache stats
  - TUI: "Profiles" tab with subsystem strip, `[d]` toggle for deviations
    overlay
  - Prometheus exporter: `profile_groups_total{subsystem}`,
    `profile_settings_total{subsystem}`, `profile_deviations_count{risk}`,
    `profile_writable_handlers_total`, `profile_cache_hits/misses_total`
- **Examples**: `examples/profile_inspector.rs`, `examples/active_profiles.rs`
- **Tests**: 55 unit tests for the profile module (525 total in the library)
- **Verified live**: 23,500 settings across 19 device groups on a Windows
  development host with RTX 3090 Ti + Micron DDR5 + Samsung NVMe.

### Fixed

- `display::DisplayInfo::aspect_ratio()` no longer panics with divide-by-zero
  on inactive virtual displays reporting 0x0 dimensions.

## [0.3.0] - 2026-02-02

### Added
- **Audio monitoring** - Device enumeration, volume control, mute status
- **Bluetooth monitoring** - Adapter and device tracking, connection states, battery levels
- **Display monitoring** - Monitor info, resolution, refresh rate, HDR support
- **USB monitoring** - Device enumeration, speed detection, class identification

### Platform Support
- Linux: Full implementations using sysfs, PulseAudio, BlueZ, xrandr
- Windows/macOS: Stub implementations ready for platform APIs

## [0.2.0] - 2026-02-02

### Added
- **Latest AI model support** - GPT-4o, GPT-4.5, o1, o3, Claude 4 Opus/Sonnet, Gemini 2.0, Grok 3, Llama 4, Mistral Large, DeepSeek-R1/V3
- **New CLI subcommand structure** - `ironmon ai query/manifest/server` and `amon query/manifest/server`
- **MCP server** - Claude Desktop integration via `ironmon ai server` or `amon server`
- **Multi-format manifest export** - openai, anthropic, gemini, grok, llama, mistral, deepseek, mcp, jsonld formats
- **AI agent export formats** - Export tool definitions for all major AI providers

### Changed  
- CLI restructured with nested subcommands for better organization
- `amon` now mirrors `ironmon ai` subcommand structure

### Fixed
- CI badge links in README (ci.yml → build-and-push.yml)
- Crates.io badge (ironmon → iron-monitor)
- Compiler warnings for unused fields

## [0.1.0] - 2026-01-30

### Added

#### Core Features
- **Cross-platform hardware monitoring** - CPU, GPU, memory, disk, network monitoring
- **Multi-vendor GPU support** - NVIDIA (NVML), AMD (ROCm/sysfs), Intel (Level Zero)
- **Process monitoring** - System processes with GPU attribution
- **Network monitoring** - Interface stats, bandwidth tracking

#### GUI Application
- **Modern egui-based GUI** with dark/light theme support
- **Real-time dashboards** - CPU, GPU, memory, disk, network visualization
- **AI chatbot integration** - Natural language system queries
- **Data export** - JSON/CSV export functionality
- **Alert system** - Configurable threshold alerts
- **Historical data** - Time-series metric storage

#### AI Integration
- **AI Data API** - 35+ tools across 8 categories for AI system visibility
- **Observability API** - MCP-like permission system for external AI access
- **Agent engine** - Context-aware query processing
- **Tool call visualization** - See what tools the AI uses

#### Observability Module
- **System context materialization** - Structured state for AI reasoning
- **Event system** - Threshold alerts, state change detection
- **Metric collection** - Time-series with aggregation (min/max/avg/percentiles)
- **Permission system** - Capability-based access control (MCP-inspired)
- **HTTP/WebSocket server** - REST API for external access
- **Real-time streaming** - WebSocket metric/event streaming

#### Platform Support
- **Linux** - Full support (procfs, sysfs, device paths)
- **Windows** - Core monitoring (Win32 API)
- **macOS** - Basic support (IOKit)

### Security
- Added `.gitignore` patterns for sensitive files
- Capability-based permission system for API access
- Rate limiting for external API requests
- Sandbox detection for telemetry consent

---

[0.1.0]: https://github.com/nervosys/IronMonitor/releases/tag/v0.1.0

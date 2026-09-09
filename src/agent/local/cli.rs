//! CLI AI providers — inference by driving a locally installed command-line tool.
//!
//! These backends shell out to a binary on `PATH` rather than speaking HTTP. That
//! makes them the lowest-friction option available: the tool is already installed and
//! already authenticated, so there is no API key to configure and no server to start.
//!
//! # Where your telemetry goes
//!
//! Being a *local process* is not the same as being *local inference*. Of the
//! supported tools, only `ollama` runs the model on your machine. `claude`, `codex`
//! and `gemini` are thin clients that relay the prompt to their vendor's hosted API,
//! so system telemetry leaves the host exactly as it would with the corresponding
//! remote backend.
//!
//! [`CliProvider::runs_on_host`] encodes that distinction, and
//! [`crate::agent::backend::BackendType::runs_on_host`] surfaces it to callers making
//! a privacy decision.
//!
//! # Cost
//!
//! Every query spawns a process. That is tens to hundreds of milliseconds of overhead
//! before the model does any work, which is why these rank below a running server in
//! [`crate::agent::backend::BackendDiscovery::recommended`].
//!
//! # Example
//!
//! ```no_run
//! use ironmonlib::agent::local::cli::{CliClient, CliProvider};
//! use ironmonlib::agent::local::{InferenceRequest, LocalInferenceClient};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Use whichever supported tool is installed.
//! if let Some(client) = CliClient::detect_any() {
//!     let response = client
//!         .generate(InferenceRequest {
//!             prompt: "Is 82C safe for a GPU under load?".to_string(),
//!             ..Default::default()
//!         })
//!         .await?;
//!     println!("{}", response.text);
//! }
//!
//! // Or pin a specific one.
//! let claude = CliClient::detect(CliProvider::Claude);
//! # Ok(())
//! # }
//! ```

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{InferenceRequest, InferenceResponse, LocalInferenceClient, ModelInfo};
use crate::error::{IronError, Result};

/// How long to wait for a CLI tool before giving up and killing it.
///
/// These tools can block indefinitely — waiting on a login prompt, a rate limit, or a
/// stalled network call — and a hung child would otherwise hang the caller forever.
const DEFAULT_CLI_TIMEOUT: Duration = Duration::from_secs(120);

/// Poll interval while waiting for the child process to exit.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Model used when Ollama is selected without an explicit model.
///
/// `ollama run` requires one positionally and cannot pick a default itself.
const DEFAULT_OLLAMA_MODEL: &str = "llama3.2";

/// A supported command-line AI tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CliProvider {
    /// Anthropic Claude Code (`claude -p`).
    Claude,
    /// OpenAI Codex CLI (`codex exec`).
    Codex,
    /// Google Gemini CLI (`gemini -p`).
    Gemini,
    /// Ollama (`ollama run <model>`). The only one that infers locally.
    Ollama,
}

impl CliProvider {
    /// Every supported provider, in detection-preference order.
    pub const ALL: [CliProvider; 4] = [
        CliProvider::Ollama,
        CliProvider::Claude,
        CliProvider::Codex,
        CliProvider::Gemini,
    ];

    /// Executable name to look for on `PATH`.
    pub fn binary(&self) -> &'static str {
        match self {
            CliProvider::Claude => "claude",
            CliProvider::Codex => "codex",
            CliProvider::Gemini => "gemini",
            CliProvider::Ollama => "ollama",
        }
    }

    /// Human-readable name.
    pub fn display_name(&self) -> &'static str {
        match self {
            CliProvider::Claude => "Claude CLI",
            CliProvider::Codex => "Codex CLI",
            CliProvider::Gemini => "Gemini CLI",
            CliProvider::Ollama => "Ollama CLI",
        }
    }

    /// Whether inference happens on this machine.
    ///
    /// Only Ollama runs a model locally. The others are clients for a hosted API and
    /// transmit the prompt — including any system telemetry embedded in it — to their
    /// vendor.
    pub fn runs_on_host(&self) -> bool {
        matches!(self, CliProvider::Ollama)
    }

    /// Whether a model name must be supplied.
    ///
    /// `ollama run` takes the model as a positional argument and cannot infer one;
    /// the others use whatever the tool is configured to use.
    pub fn requires_model(&self) -> bool {
        matches!(self, CliProvider::Ollama)
    }

    /// Locate this tool on `PATH`, if installed.
    pub fn detect(&self) -> Option<PathBuf> {
        which_on_path(self.binary())
    }

    /// Build the argument list for a non-interactive, single-shot query.
    ///
    /// The prompt is passed as a distinct argument rather than interpolated into a
    /// shell string, so no quoting or escaping is involved and prompt content cannot
    /// be interpreted as shell syntax.
    fn build_args(&self, prompt: &str, model: Option<&str>) -> Vec<String> {
        match self {
            // `-p/--print` is Claude Code's non-interactive mode; without it the tool
            // starts an interactive session and never returns.
            CliProvider::Claude => {
                let mut args = vec!["-p".to_string(), prompt.to_string()];
                if let Some(model) = model {
                    args.push("--model".to_string());
                    args.push(model.to_string());
                }
                args
            }
            // `codex exec` is the non-interactive subcommand.
            CliProvider::Codex => {
                let mut args = vec!["exec".to_string()];
                if let Some(model) = model {
                    args.push("--model".to_string());
                    args.push(model.to_string());
                }
                args.push(prompt.to_string());
                args
            }
            CliProvider::Gemini => {
                let mut args = vec!["-p".to_string(), prompt.to_string()];
                if let Some(model) = model {
                    args.push("--model".to_string());
                    args.push(model.to_string());
                }
                args
            }
            // `ollama run MODEL PROMPT`. --hidethinking keeps reasoning traces out of
            // the answer for models that emit them.
            CliProvider::Ollama => vec![
                "run".to_string(),
                model.unwrap_or(DEFAULT_OLLAMA_MODEL).to_string(),
                prompt.to_string(),
                "--hidethinking".to_string(),
            ],
        }
    }
}

impl std::fmt::Display for CliProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Whether `binary` exists somewhere on `PATH`.
///
/// Exposed for other backends whose availability is also "is the tool installed" —
/// llama.cpp, for instance, is driven through its CLI.
pub fn binary_on_path(binary: &str) -> bool {
    which_on_path(binary).is_some()
}

/// Look up an executable on `PATH`.
///
/// Uses `std::env::var_os("PATH")` directly rather than shelling out to `where`/
/// `which`, which would spawn a process just to find one.
fn which_on_path(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;

    // On Windows an executable may carry any of the PATHEXT suffixes; a bare name
    // usually will not exist on disk.
    #[cfg(target_os = "windows")]
    let candidates: Vec<String> = {
        let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_string());
        let mut names = vec![binary.to_string()];
        names.extend(
            exts.split(';')
                .filter(|e| !e.is_empty())
                .map(|ext| format!("{binary}{}", ext.to_lowercase())),
        );
        names
    };
    #[cfg(not(target_os = "windows"))]
    let candidates: Vec<String> = vec![binary.to_string()];

    for dir in std::env::split_paths(&path) {
        for name in &candidates {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// A client that drives a command-line AI tool.
#[derive(Debug, Clone)]
pub struct CliClient {
    provider: CliProvider,
    binary: PathBuf,
    timeout: Duration,
}

impl CliClient {
    /// Build a client for `provider`, if its binary is installed.
    pub fn detect(provider: CliProvider) -> Option<Self> {
        provider.detect().map(|binary| Self {
            provider,
            binary,
            timeout: DEFAULT_CLI_TIMEOUT,
        })
    }

    /// Build a client for the first supported tool found on `PATH`.
    ///
    /// Ollama is tried first because it is the only one that keeps telemetry on the
    /// host; see [`CliProvider::ALL`].
    pub fn detect_any() -> Option<Self> {
        CliProvider::ALL.iter().copied().find_map(Self::detect)
    }

    /// Every supported tool currently installed.
    pub fn detect_all() -> Vec<Self> {
        CliProvider::ALL
            .iter()
            .copied()
            .filter_map(Self::detect)
            .collect()
    }

    /// Override how long to wait before killing the child process.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Which tool this client drives.
    pub fn provider(&self) -> CliProvider {
        self.provider
    }

    /// Resolved path of the executable.
    pub fn binary_path(&self) -> &PathBuf {
        &self.binary
    }

    /// Run a single-shot query synchronously.
    ///
    /// The async [`LocalInferenceClient::generate`] delegates here. This is exposed
    /// separately because the agent's [`crate::agent::remote::RemoteClient`] path is
    /// blocking and would otherwise need an async runtime just to spawn a process.
    ///
    /// Returns the model's text, and the wall-clock duration in milliseconds.
    pub fn query_blocking(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        model: Option<&str>,
    ) -> Result<(String, u64)> {
        let start = Instant::now();

        // These tools accept one prompt with no separate system channel.
        let prompt = if system_prompt.is_empty() {
            user_prompt.to_string()
        } else {
            format!("{system_prompt}\n\n{user_prompt}")
        };

        let model = model.filter(|m| !m.is_empty() && *m != "default").or(
            if self.provider.requires_model() {
                Some(DEFAULT_OLLAMA_MODEL)
            } else {
                None
            },
        );

        let text = self.run(&self.provider.build_args(&prompt, model))?;
        Ok((text, start.elapsed().as_millis() as u64))
    }

    /// Run the tool and capture stdout, enforcing the timeout.
    ///
    /// Returns an error rather than blocking forever if the tool hangs — these are
    /// interactive-first programs and will happily wait on a prompt no one is there
    /// to answer.
    fn run(&self, args: &[String]) -> Result<String> {
        let mut child = Command::new(&self.binary)
            .args(args)
            // Close stdin so a tool that decides to prompt gets EOF and exits, rather
            // than waiting on input that will never arrive.
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                IronError::Agent(format!("failed to start {}: {e}", self.binary.display()))
            })?;

        let deadline = Instant::now() + self.timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        // Best-effort cleanup; the error below is the real result.
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(IronError::Agent(format!(
                            "{} did not respond within {}s",
                            self.provider.display_name(),
                            self.timeout.as_secs()
                        )));
                    }
                    std::thread::sleep(POLL_INTERVAL);
                }
                Err(e) => {
                    return Err(IronError::Agent(format!(
                        "failed while waiting on {}: {e}",
                        self.provider.display_name()
                    )))
                }
            }
        }

        let output = child.wait_with_output().map_err(|e| {
            IronError::Agent(format!(
                "failed to read output from {}: {e}",
                self.provider.display_name()
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(IronError::Agent(format!(
                "{} exited with {}: {}",
                self.provider.display_name(),
                output.status,
                stderr.trim()
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

#[async_trait]
impl LocalInferenceClient for CliClient {
    fn name(&self) -> &str {
        self.provider.display_name()
    }

    /// The binary was located at construction, so this re-checks that it is still
    /// present — a tool can be uninstalled while ironmon runs.
    async fn is_available(&self) -> bool {
        self.binary.is_file()
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Only Ollama exposes a machine-readable model list without a round trip to a
        // hosted API. For the rest, the model is whatever the tool is configured for,
        // which IronMonitor cannot enumerate.
        if self.provider != CliProvider::Ollama {
            return Ok(Vec::new());
        }

        let output = self.run(&["list".to_string()])?;
        Ok(output
            .lines()
            .skip(1) // header row
            .filter_map(|line| line.split_whitespace().next())
            .filter(|name| !name.is_empty())
            .map(|name| ModelInfo {
                name: name.to_string(),
                size: None,
                family: None,
                parameter_count: None,
                quantization: None,
            })
            .collect())
    }

    async fn generate(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        let (text, duration_ms) = self.query_blocking(
            request.system.as_deref().unwrap_or(""),
            &request.prompt,
            Some(request.model.as_str()),
        )?;

        Ok(InferenceResponse {
            text,
            model: if request.model.is_empty() {
                "default".to_string()
            } else {
                request.model
            },
            // CLI tools do not report token counts on stdout.
            tokens_generated: None,
            duration_ms,
            // No way to distinguish a natural stop from a truncated one here.
            truncated: false,
        })
    }

    async fn model_info(&self, model_name: &str) -> Result<ModelInfo> {
        let models = self.list_models().await?;
        models
            .into_iter()
            .find(|m| m.name == model_name)
            .ok_or_else(|| {
                IronError::Agent(format!(
                    "{} does not report a model named {model_name}",
                    self.provider.display_name()
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ollama_infers_on_host() {
        // The other three are thin clients for a hosted API. Getting this wrong would
        // route sensitive telemetry off-machine while reporting it as local.
        assert!(CliProvider::Ollama.runs_on_host());
        assert!(!CliProvider::Claude.runs_on_host());
        assert!(!CliProvider::Codex.runs_on_host());
        assert!(!CliProvider::Gemini.runs_on_host());
    }

    #[test]
    fn ollama_is_preferred_in_detection_order() {
        // Ollama is the only on-host option, so it must be tried first by
        // `detect_any`.
        assert_eq!(CliProvider::ALL[0], CliProvider::Ollama);
        assert!(CliProvider::ALL[0].runs_on_host());
    }

    #[test]
    fn non_interactive_flags_are_present() {
        // Without these each tool starts an interactive session and never returns,
        // which would hang the agent until the timeout fires.
        let claude = CliProvider::Claude.build_args("hi", None);
        assert_eq!(claude.first().map(String::as_str), Some("-p"));

        let codex = CliProvider::Codex.build_args("hi", None);
        assert_eq!(codex.first().map(String::as_str), Some("exec"));

        let gemini = CliProvider::Gemini.build_args("hi", None);
        assert_eq!(gemini.first().map(String::as_str), Some("-p"));

        let ollama = CliProvider::Ollama.build_args("hi", Some("llama3.2"));
        assert_eq!(ollama.first().map(String::as_str), Some("run"));
    }

    #[test]
    fn prompt_is_a_separate_argument_never_shell_interpolated() {
        // Passing the prompt as its own argv entry means shell metacharacters are
        // inert. If this ever became a formatted shell string, a prompt containing
        // hardware data could execute commands.
        let nasty = "test\"; rm -rf /; echo \"";
        for provider in CliProvider::ALL {
            let args = provider.build_args(nasty, Some("m"));
            assert!(
                args.iter().any(|a| a == nasty),
                "{provider} must pass the prompt verbatim as one argument"
            );
        }
    }

    #[test]
    fn ollama_requires_a_model_and_others_do_not() {
        assert!(CliProvider::Ollama.requires_model());
        assert!(!CliProvider::Claude.requires_model());

        // `ollama run` takes the model positionally, so it must appear before the
        // prompt.
        let args = CliProvider::Ollama.build_args("prompt", Some("qwen3"));
        let model_idx = args.iter().position(|a| a == "qwen3").expect("model");
        let prompt_idx = args.iter().position(|a| a == "prompt").expect("prompt");
        assert!(model_idx < prompt_idx, "model must precede the prompt");
    }

    #[test]
    fn detection_reports_absent_tools_as_none() {
        // A provider whose binary is not installed must be absent, not a client that
        // fails at query time.
        for provider in CliProvider::ALL {
            match CliClient::detect(provider) {
                Some(client) => {
                    // Printed so `--nocapture` shows what this machine actually has,
                    // which is otherwise invisible in a pass/fail result.
                    eprintln!("detected {provider} -> {}", client.binary_path().display());
                    assert!(
                        client.binary_path().is_file(),
                        "{provider} was detected but its binary does not exist"
                    );
                }
                None => {
                    eprintln!("absent   {provider}");
                    assert!(
                        provider.detect().is_none(),
                        "{provider} detection disagreed with itself"
                    );
                }
            }
        }
    }

    /// Exercise the real subprocess machinery end to end against a live Ollama.
    ///
    /// `ollama list` is used rather than a generation because it needs no model
    /// pulled, while still covering spawn, the timeout loop, stdout capture,
    /// exit-status handling and parsing.
    ///
    /// Opt-in via `IRONMON_TEST_EXTERNAL_CLI=1`. It talks to a background service whose
    /// response time is not under this suite's control: run in parallel with the rest
    /// of the tests on a loaded machine, `ollama list` has taken anywhere from 0.4s to
    /// long enough to look like a hang, which made `cargo test` flaky and once wedged
    /// it for the better part of an hour. A test that fails because another program
    /// was busy is not testing this crate.
    ///
    /// The timeout is also cut from the 120s default to 10s, so an opt-in run that
    /// does go wrong fails fast rather than idling.
    #[tokio::test]
    async fn ollama_subprocess_round_trip() {
        if std::env::var_os("IRONMON_TEST_EXTERNAL_CLI").is_none() {
            eprintln!("skipping: set IRONMON_TEST_EXTERNAL_CLI=1 to run against a live ollama");
            return;
        }

        let Some(client) = CliClient::detect(CliProvider::Ollama)
            .map(|client| client.with_timeout(Duration::from_secs(10)))
        else {
            eprintln!("skipping: ollama not installed");
            return;
        };

        assert!(client.is_available().await);

        let models = client
            .list_models()
            .await
            .expect("`ollama list` should succeed when ollama is installed");

        eprintln!("ollama reports {} model(s)", models.len());
        for model in &models {
            assert!(
                !model.name.is_empty(),
                "parsed an empty model name from `ollama list` output"
            );
            // The header row must have been skipped, not parsed as a model.
            assert_ne!(model.name.to_uppercase(), "NAME");
        }
    }

    /// A tool that hangs must be killed rather than blocking the caller forever.
    #[test]
    fn timeout_kills_a_hung_child() {
        // Every platform ships something that sleeps. Use the shell so this needs no
        // fixture binary.
        #[cfg(target_os = "windows")]
        let (bin, args) = ("cmd", vec!["/c".to_string(), "timeout /t 30".to_string()]);
        #[cfg(not(target_os = "windows"))]
        let (bin, args) = ("sh", vec!["-c".to_string(), "sleep 30".to_string()]);

        let Some(binary) = which_on_path(bin) else {
            eprintln!("skipping: {bin} not found");
            return;
        };

        let client = CliClient {
            provider: CliProvider::Ollama,
            binary,
            timeout: Duration::from_millis(300),
        };

        let started = Instant::now();
        let result = client.run(&args);
        let elapsed = started.elapsed();

        assert!(result.is_err(), "a hung child must produce an error");
        assert!(
            elapsed < Duration::from_secs(10),
            "timeout did not fire; waited {elapsed:?} for a 300ms budget"
        );
    }

    #[test]
    fn which_on_path_finds_a_known_binary() {
        // cargo is running this test, so it is on PATH by construction.
        let found = which_on_path("cargo");
        assert!(
            found.is_some(),
            "PATH lookup failed for a binary known to exist"
        );
        assert!(found.unwrap().is_file());
    }

    #[test]
    fn which_on_path_rejects_nonexistent_binary() {
        assert!(which_on_path("ironmon-definitely-not-a-real-binary-xyzzy").is_none());
    }
}

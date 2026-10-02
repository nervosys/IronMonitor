//! Bounded model-directed, read-only monitoring tool calls.
use super::{BackendType, RemoteClient};
use crate::ai_api::{AiDataApi, ToolDefinition};
use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, OnceLock,
};
use std::time::{Duration, Instant};

const MAX_ROUNDS: usize = 6;
const MAX_CALLS: usize = 16;
const RESULT_BYTES: usize = 32 * 1024;
const TOTAL_BYTES: usize = 256 * 1024;
const RESPONSE_BYTES: usize = 512 * 1024;
const HISTORY_BYTES: usize = 16 * 1024;
const HISTORY_TURNS: usize = 6;

/// A completed exchange used to resolve conversational references, not as a
/// source of current observations. Failed or cancelled exchanges are omitted.
#[derive(Debug, Clone)]
pub struct ConversationTurn {
    pub user: String,
    pub assistant: String,
}

pub(crate) fn explanation_followup(question: &str, history: &[ConversationTurn]) -> bool {
    if !history.last().is_some_and(|turn| {
        !turn.user.trim().is_empty()
            && !turn.assistant.trim().is_empty()
            && turn.user.len().saturating_add(turn.assistant.len()) <= HISTORY_BYTES
    }) {
        return false;
    }
    let normalized = question
        .trim()
        .trim_end_matches(['?', '.', '!'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "what is that"
            | "what's that"
            | "what is it"
            | "what does that mean"
            | "explain that"
            | "tell me more"
            | "can you explain that"
    )
}

fn conversation_messages(history: &[ConversationTurn]) -> Vec<Value> {
    let mut bytes = 0;
    let mut recent = Vec::new();
    for turn in history.iter().rev().take(HISTORY_TURNS) {
        let size = turn.user.len().saturating_add(turn.assistant.len());
        if size > HISTORY_BYTES - bytes {
            break;
        }
        if turn.user.trim().is_empty() || turn.assistant.trim().is_empty() {
            break;
        }
        bytes += size;
        recent.push(turn);
    }
    recent
        .into_iter()
        .rev()
        .flat_map(|turn| {
            [
                json!({"role":"user","content":turn.user}),
                json!({"role":"assistant","content":turn.assistant}),
            ]
        })
        .collect()
}
const READ_TOOLS: [&str; 10] = [
    "describe_entities",
    "get_observation_snapshot",
    "get_collector_health",
    "query_metric_history",
    "query_events",
    "inspect_process",
    "check_endpoint",
    "list_connections",
    "inspect_services",
    "query_os_events",
];
const LEGACY_READ: &[&str] = &[
    "get_system_summary",
    "get_system_info",
    "get_platform_info",
    "get_gpu_status",
    "get_gpu_list",
    "get_gpu_details",
    "get_gpu_processes",
    "get_gpu_utilization",
    "get_gpu_memory",
    "get_gpu_temperature",
    "get_gpu_power",
    "get_cpu_status",
    "get_cpu_cores",
    "get_cpu_frequency",
    "get_memory_status",
    "get_memory_breakdown",
    "get_swap_status",
    "get_disk_list",
    "get_disk_details",
    "get_disk_io",
    "get_disk_health",
    "get_network_interfaces",
    "get_network_bandwidth",
    "get_interface_details",
    "get_process_list",
    "get_process_details",
    "get_top_cpu_processes",
    "get_top_memory_processes",
    "get_top_gpu_processes",
    "search_processes",
    "get_motherboard_sensors",
    "get_system_temperatures",
    "get_fan_speeds",
    "get_voltage_rails",
    "get_driver_info",
    "list_profile_subsystems",
    "get_profile_settings",
    "get_profile_deviations",
    "explain_profile_setting",
    "get_active_app_profiles",
    "search_profile_settings",
];
thread_local! {static LEGACY_API: std::cell::RefCell<Option<AiDataApi>> = const {std::cell::RefCell::new(None)};}

/// Progress contains tool names and status, never raw arguments or log contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolActivity {
    pub tool: String,
    pub phase: String,
    pub elapsed_ms: u64,
}
/// Per-run cancellation and optional bounded UI progress channel.
#[derive(Clone, Default)]
pub struct RunControl {
    cancelled: Arc<AtomicBool>,
    progress: Option<mpsc::SyncSender<ToolActivity>>,
}
impl RunControl {
    pub fn with_progress(sender: mpsc::SyncSender<ToolActivity>) -> Self {
        Self {
            progress: Some(sender),
            ..Default::default()
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    fn check(&self, deadline: Instant) -> Result<()> {
        if self.is_cancelled() {
            return Err(IronError::Agent("Agent run cancelled".into()));
        }
        if Instant::now() >= deadline {
            return Err(IronError::Agent(
                "Agent run exceeded its overall deadline".into(),
            ));
        }
        Ok(())
    }
    fn report(&self, tool: &str, phase: &str, elapsed: Duration) {
        if let Some(sender) = &self.progress {
            let _ = sender.try_send(ToolActivity {
                tool: tool.into(),
                phase: phase.into(),
                elapsed_ms: elapsed.as_millis() as u64,
            });
        }
    }
}

fn definitions() -> Vec<ToolDefinition> {
    AiDataApi::with_components(None, None, None)
        .list_tools()
        .into_iter()
        .filter(|t| READ_TOOLS.contains(&t.name.as_str()) || LEGACY_READ.contains(&t.name.as_str()))
        .collect()
}
struct Work {
    task: Box<dyn FnOnce() -> Result<Value> + Send>,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
    reply: mpsc::SyncSender<Result<Value>>,
}
static HTTP_WORK: OnceLock<mpsc::SyncSender<Work>> = OnceLock::new();
static TOOL_WORK: OnceLock<mpsc::SyncSender<Work>> = OnceLock::new();
static LEGACY_WORK: OnceLock<mpsc::SyncSender<Work>> = OnceLock::new();

// Fixed workers and bounded queues avoid orphaning an unbounded number of
// threads when a provider stalls. Cancellation releases the caller promptly.
fn work(
    queue: &'static OnceLock<mpsc::SyncSender<Work>>,
    name: &str,
    task: impl FnOnce() -> Result<Value> + Send + 'static,
    control: &RunControl,
    deadline: Instant,
) -> Result<Value> {
    control.check(deadline)?;
    let sender = queue.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<Work>(2);
        let _ = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    let result = if job.cancelled.load(Ordering::Acquire)
                        || Instant::now() >= job.deadline
                    {
                        Err(IronError::Agent(
                            "Queued agent work was cancelled or expired".into(),
                        ))
                    } else {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(job.task))
                            .unwrap_or_else(|_| Err(IronError::Agent("Agent worker failed".into())))
                    };
                    let _ = job.reply.send(result);
                }
            });
        tx
    });
    let (reply, rx) = mpsc::sync_channel(1);
    sender
        .try_send(Work {
            task: Box::new(task),
            cancelled: control.cancelled.clone(),
            deadline,
            reply,
        })
        .map_err(|_| IronError::Agent("Agent worker queue is full or unavailable".into()))?;
    loop {
        control.check(deadline)?;
        match rx.recv_timeout(
            Duration::from_millis(50).min(deadline.saturating_duration_since(Instant::now())),
        ) {
            Ok(value) => {
                control.check(deadline)?;
                return value;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(IronError::Agent("Agent worker disconnected".into()))
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Wire {
    Chat,
    Ollama,
    Anthropic,
    Responses,
}
struct Call {
    id: String,
    name: String,
    args: Value,
}

fn validate_args(value: &Value, schema: &Value, depth: usize) -> std::result::Result<(), String> {
    if depth > 8 {
        return Err("Tool arguments exceed eight nesting levels".into());
    }
    if let Some(options) = schema["enum"].as_array() {
        if !options.contains(value) {
            return Err("Argument is not a declared choice".into());
        }
    }
    let valid = match schema["type"].as_str() {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("integer") => value.is_i64() || value.is_u64(),
        Some("number") => value.is_number(),
        Some("boolean") => value.is_boolean(),
        _ => true,
    };
    if !valid {
        return Err(format!(
            "Expected JSON type {}",
            schema["type"].as_str().unwrap_or("declared type")
        ));
    }
    if let Some(number) = value.as_f64() {
        if schema["minimum"].as_f64().is_some_and(|min| number < min)
            || schema["maximum"].as_f64().is_some_and(|max| number > max)
        {
            return Err("Numeric argument is outside its declared range".into());
        }
    }
    if let Some(text) = value.as_str() {
        if text.len() > schema["maxLength"].as_u64().unwrap_or(2048) as usize
            || text.chars().count() < schema["minLength"].as_u64().unwrap_or(0) as usize
        {
            return Err("String argument exceeds its declared bounds".into());
        }
    }
    if let Some(array) = value.as_array() {
        if array.len() > schema["maxItems"].as_u64().unwrap_or(128) as usize
            || array.len() < schema["minItems"].as_u64().unwrap_or(0) as usize
        {
            return Err("Array argument exceeds its declared bounds".into());
        }
        for item in array {
            validate_args(item, &schema["items"], depth + 1)?;
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema["required"].as_array() {
            for field in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(field) {
                    return Err(format!("Missing required argument: {field}"));
                }
            }
        }
        for (key, item) in object {
            let property = schema["properties"].get(key).ok_or_else(|| {
                format!(
                    "Unrecognized argument: {key}; allowed arguments: {}",
                    schema["properties"]
                        .as_object()
                        .map(|p| p.keys().cloned().collect::<Vec<_>>().join(", "))
                        .unwrap_or_default()
                )
            })?;
            validate_args(item, property, depth + 1).map_err(|error| format!("{key}: {error}"))?;
        }
    }
    Ok(())
}

fn parse_calls(message: &Value, wire: Wire) -> Result<Vec<Call>> {
    let empty = Vec::new();
    let rows = match wire {
        Wire::Anthropic => message["content"].as_array().unwrap_or(&empty),
        Wire::Responses => message["output"].as_array().unwrap_or(&empty),
        _ => message["tool_calls"].as_array().unwrap_or(&empty),
    };
    let mut calls = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if wire == Wire::Anthropic && row["type"] != "tool_use" {
            continue;
        }
        if wire == Wire::Responses && row["type"] != "function_call" {
            continue;
        }
        if calls.len() >= MAX_CALLS {
            return Err(IronError::Agent(
                "Model exceeded the tool-call batch limit".into(),
            ));
        }
        let (name, args, id) = match wire {
            Wire::Anthropic => (
                row["name"].as_str(),
                row["input"].clone(),
                row["id"].as_str().map(str::to_owned),
            ),
            Wire::Responses => (
                row["name"].as_str(),
                row["arguments"].clone(),
                row["call_id"].as_str().map(str::to_owned),
            ),
            _ => (
                row["function"]["name"].as_str(),
                row["function"]["arguments"].clone(),
                row["id"].as_str().map(str::to_owned),
            ),
        };
        let name = name
            .filter(|name| !name.is_empty() && name.len() <= 64)
            .ok_or_else(|| IronError::Agent("Malformed tool name".into()))?;
        let args = if let Some(text) = args.as_str() {
            if text.len() > 8192 {
                return Err(IronError::Agent("Tool arguments exceed 8192 bytes".into()));
            }
            serde_json::from_str(text)
                .map_err(|e| IronError::Agent(format!("Malformed tool arguments: {e}")))?
        } else {
            args
        };
        if !args.is_object() || serde_json::to_vec(&args).map_or(true, |v| v.len() > 8192) {
            return Err(IronError::Agent(
                "Tool arguments must be an object of at most 8192 bytes".into(),
            ));
        }
        let id = match id {
            Some(id) => id,
            None if wire == Wire::Ollama => format!("ollama-{index}"),
            None => {
                return Err(IronError::Agent(
                    "Tool call did not contain its result ID".into(),
                ))
            }
        };
        if id.is_empty() || id.len() > 128 || calls.iter().any(|c: &Call| c.id == id) {
            return Err(IronError::Agent("Invalid or repeated tool-call ID".into()));
        }
        calls.push(Call {
            id,
            name: name.into(),
            args,
        });
    }
    Ok(calls)
}

/// Some local checkpoints emit the complete call as JSON content instead
/// of the server's native call envelope. Accept only an exact, known read call;
/// never extract fragments from prose or repair malformed JSON.
fn normalize_ironworks_call(message: &mut Value, round: usize) -> Result<()> {
    normalize_local_call(message, round, "IronWorks", false)
}

/// Ollama's small checkpoints sometimes encode integer arguments as strings.
/// Convert only canonical unsigned decimal strings in declared integer fields;
/// validation still checks types, choices and bounds before any tool executes.
fn normalize_ollama_integer_args(args: &mut Value, schema: &Value) {
    let Some(object) = args.as_object_mut() else {
        return;
    };
    for (name, value) in object {
        if schema["properties"][name]["type"] != "integer" {
            continue;
        }
        let Some(text) = value.as_str() else {
            continue;
        };
        if let Ok(number) = text.parse::<u64>() {
            if number.to_string() == text {
                *value = json!(number);
            }
        }
    }
}

fn normalize_local_call(
    message: &mut Value,
    round: usize,
    provider: &str,
    allow_parameters: bool,
) -> Result<()> {
    if message["tool_calls"]
        .as_array()
        .is_some_and(|calls| !calls.is_empty())
    {
        return Ok(());
    }
    let text = message["content"].as_str().unwrap_or("").trim();
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        if text.starts_with('{')
            && text.contains("\"name\"")
            && (text.contains("\"arguments\"")
                || (allow_parameters && text.contains("\"parameters\"")))
        {
            return Err(IronError::Agent(format!(
                "{provider} returned malformed tool-call syntax; no tool was run"
            )));
        }
        return Ok(());
    };
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    let Some(name) = object.get("name").and_then(Value::as_str) else {
        return Ok(());
    };
    if !READ_TOOLS.contains(&name) && !LEGACY_READ.contains(&name) {
        return Ok(());
    }
    let argument_key = if object.contains_key("arguments") {
        "arguments"
    } else if allow_parameters && object.contains_key("parameters") {
        "parameters"
    } else {
        return Ok(());
    };
    if object.len() != 2 {
        return Err(IronError::Agent(format!(
            "{provider} returned ambiguous tool-call syntax; no tool was run"
        )));
    }
    // parse_calls and tool_output enforce the normal argument, schema and
    // read-only limits after normalization.
    *message = json!({"role":"assistant","content":null,"tool_calls":[{
        "id":format!("{}-text-{round}",provider.to_ascii_lowercase()),"type":"function",
        "function":{"name":name,"arguments":value[argument_key]}
    }]});
    Ok(())
}

// Small local models can identify the right process but corrupt its numeric
// value when composing prose. Render simple RAM-leader answers from the actual
// model-requested ranking result; complex questions retain model synthesis.
fn simple_ram_ranking_question(question: &str) -> bool {
    let lower = question.to_ascii_lowercase();
    let words: Vec<_> = lower
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|word| !word.is_empty())
        .collect();
    matches!(words.first(), Some(&"what" | &"which"))
        && words.iter().any(|word| ["ram", "memory"].contains(word))
        && words
            .iter()
            .any(|word| ["most", "largest", "highest"].contains(word))
        && !words.iter().any(|word| {
            [
                "and", "why", "how", "compare", "trend", "over", "not", "least", "gpu", "vram",
                "swap", "disk", "cpu",
            ]
            .contains(word)
        })
}

fn observed_ram_leader(result: &Value) -> Option<String> {
    if result["success"] != true {
        return None;
    }
    let first = result["data"].as_array()?.first()?;
    let name = first["name"].as_str()?;
    let bytes = first["memory_bytes"].as_u64()?;
    // Quoting keeps process names containing line breaks distinguishable from
    // answer text. Units and numbers come from the reading, never model prose.
    Some(format!(
        "{} is using the most resident RAM: {} MiB.",
        serde_json::to_string(name).ok()?,
        bytes / 1024 / 1024
    ))
}

#[cfg(feature = "remote-backends")]
fn ollama_retry_thinking(metadata: &Value) -> Option<Value> {
    let values = metadata["thinking"]["values"].as_array()?;
    if values.contains(&json!(false)) {
        Some(json!(false))
    } else if values.contains(&json!("low")) {
        Some(json!("low"))
    } else {
        None
    }
}

fn content(message: &Value, wire: Wire) -> String {
    match wire {
        Wire::Responses => message["output"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| item["type"] == "message")
            .flat_map(|item| item["content"].as_array().into_iter().flatten())
            .filter_map(|item| item["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        Wire::Anthropic => message["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => message["content"].as_str().unwrap_or_default().to_string(),
    }
}

fn tool_output(
    call: Call,
    control: &RunControl,
    deadline: Instant,
    remaining_bytes: usize,
) -> Result<String> {
    if !READ_TOOLS.contains(&call.name.as_str()) && !LEGACY_READ.contains(&call.name.as_str()) {
        control.report("Unregistered tool", "refused", Duration::ZERO);
        return Ok(json!({"success":false,"error":"Tool is not in the built-in agent's read-only registry"}).to_string());
    }
    let started = Instant::now();
    let definition = definitions()
        .into_iter()
        .find(|tool| tool.name == call.name)
        .ok_or_else(|| IronError::Agent("Tool definition missing".into()))?;
    if let Err(error) = validate_args(&call.args, &definition.parameters, 0) {
        control.report(&call.name, "invalid arguments", Duration::ZERO);
        return Ok(json!({"success":false,"tool_name":call.name,"error":error,
            "expected_parameters":definition.parameters,
            "instruction":"Correct the arguments and call the tool again. Integer fields require JSON numbers, not strings. Do not invent tool output."}).to_string());
    }
    control.report(&call.name, "running", Duration::ZERO);
    let name = call.name.clone();
    let mut args = call.args;
    let tool_deadline = deadline.min(Instant::now() + Duration::from_secs(5));
    if name == "check_endpoint" {
        let timeout = args
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(3000);
        // Validate the declared upper bound before applying the run budget.
        if (100..=10000).contains(&timeout) {
            args["timeout_ms"] = json!(timeout
                .min(
                    tool_deadline
                        .saturating_duration_since(Instant::now())
                        .as_millis() as u64
                )
                .max(100));
        }
    }
    let legacy = LEGACY_READ.contains(&name.as_str());
    let result = work(
        if legacy { &LEGACY_WORK } else { &TOOL_WORK },
        if legacy {
            "agent-legacy-worker"
        } else {
            "agent-tool-worker"
        },
        move || {
            let result = if legacy {
                LEGACY_API.with(|cell| {
                    let mut api = cell.borrow_mut();
                    if api.is_none() {
                        *api = Some(AiDataApi::new()?);
                    }
                    api.as_mut()
                        .ok_or_else(|| IronError::Agent("Monitoring API unavailable".into()))?
                        .call_tool(&name, args)
                })?
            } else {
                AiDataApi::with_components(None, None, None).call_tool(&name, args)?
            };
            serde_json::to_value(result).map_err(|e| IronError::Agent(e.to_string()))
        },
        control,
        tool_deadline,
    );
    control.check(deadline)?;
    let value = match result {
        Ok(value) => value,
        Err(error) => json!({"success":false,"error":error.to_string()}),
    };
    control.report(
        &call.name,
        if value["success"] == false {
            "failed"
        } else {
            "completed"
        },
        started.elapsed(),
    );
    let output = serde_json::to_string(&value).map_err(|e| IronError::Agent(e.to_string()))?;
    if output.len() > RESULT_BYTES.min(remaining_bytes) {
        return Ok(json!({"success":false,"error":"Tool result exceeded the agent context budget; retry with narrower filters or a smaller limit","withheld_bytes":output.len()}).to_string());
    }
    Ok(output)
}

impl RemoteClient {
    /// Model-directed tools share a registry with MCP and use an overall deadline.
    pub fn query_with_tools(
        &self,
        system_prompt: &str,
        user_query: &str,
        budget: Duration,
        control: &RunControl,
    ) -> Result<String> {
        self.query_with_tools_and_history(system_prompt, user_query, &[], budget, control)
    }

    /// Supply bounded completed exchanges while taking fresh tool observations.
    pub fn query_with_tools_and_history(
        &self,
        system_prompt: &str,
        user_query: &str,
        history: &[ConversationTurn],
        budget: Duration,
        control: &RunControl,
    ) -> Result<String> {
        let deadline = Instant::now() + budget.min(Duration::from_secs(120));
        if user_query.len() > 16384 {
            return Err(IronError::Agent("Question exceeds 16 KiB".into()));
        }
        if crate::consent::is_offline_mode() && !self.config.backend_type.runs_on_host() {
            return Err(IronError::Agent(
                "Refusing an off-host backend in offline mode".into(),
            ));
        }
        control.check(deadline)?;
        // CLI providers expose a text interface, not this application's tool protocol.
        if matches!(self.config.backend_type, BackendType::Cli(_)) {
            control.report(
                "Backend",
                "text-only; native monitoring tools unavailable",
                Duration::ZERO,
            );
            let mut config = self.config.clone();
            config.timeout = deadline.saturating_duration_since(Instant::now());
            let snapshot = tool_output(
                Call {
                    id: "fallback".into(),
                    name: "get_observation_snapshot".into(),
                    args: json!({"wait_ms":1000}),
                },
                control,
                deadline,
                RESULT_BYTES,
            )?;
            let system=format!("{system_prompt}\nThis backend is text-only; it cannot request native monitoring tools. Available observations:\n{snapshot}");
            let prior = conversation_messages(history);
            let query = if prior.is_empty() {
                user_query.to_string()
            } else {
                format!("Previous conversation (reference context only):\n{}\nCurrent question:\n{user_query}", serde_json::to_string(&prior)?)
            };
            let result = work(
                &HTTP_WORK,
                "agent-http-worker",
                move || {
                    let client = RemoteClient::new(config)?;
                    let (text, _) = client.query(&system, &query)?;
                    Ok(json!(text))
                },
                control,
                deadline,
            )?;
            return Ok(result.as_str().unwrap_or_default().into());
        }
        #[cfg(not(feature = "remote-backends"))]
        {
            let _ = (system_prompt, history);
            Err(IronError::NotImplemented(
                "Agent tool calling requires remote-backends".into(),
            ))
        }
        #[cfg(feature = "remote-backends")]
        {
            let wire = match self.config.backend_type {
                BackendType::RemoteOllama => Wire::Ollama,
                BackendType::RemoteAnthropic => Wire::Anthropic,
                BackendType::RemoteOpenAI => Wire::Responses,
                _ => Wire::Chat,
            };
            let catalog = definitions();
            let tools:Vec<_>=catalog.iter().map(|t|match wire {
                Wire::Anthropic=>json!({"name":t.name,"description":t.description,"input_schema":t.parameters}),
                Wire::Responses=>json!({"type":"function","name":t.name,"description":t.description,"parameters":t.parameters,"strict":false}),
                _=>json!({"type":"function","function":{"name":t.name,"description":t.description,"parameters":t.parameters}}),
            }).collect();
            let mut messages = Vec::new();
            if wire != Wire::Anthropic {
                messages.push(json!({"role":"system","content":system_prompt}));
            }
            messages.extend(conversation_messages(history));
            messages.push(json!({"role":"user","content":user_query}));
            let mut used = 0;
            let mut successful_calls = 0;
            let render_ram_leader = simple_ram_ranking_question(user_query);
            let mut ram_leader = None;
            let explaining = explanation_followup(user_query, history);
            let mut bytes = 0;
            let mut fingerprints = Vec::new();
            let mut final_only = false;
            let mut empty_retry = false;
            let mut retry_think: Option<Value> = None;
            for round in 0..MAX_ROUNDS {
                control.check(deadline)?;
                let answer_only = matches!(
                    self.config.backend_type,
                    BackendType::IronWorks | BackendType::RemoteOllama
                ) && (final_only || round + 1 == MAX_ROUNDS);
                if answer_only {
                    messages.push(json!({"role":"user","content":"The tool budget is exhausted or the same request has repeated. Give a final answer using the tool results already supplied. Explain failed or unavailable readings and evidence limits. Do not request another tool or output tool-call syntax."}));
                }
                let mut body = match wire {
                    Wire::Responses => {
                        json!({"model":self.config.model_id,"input":messages,"tools":tools,"max_output_tokens":self.config.max_tokens,"store":false})
                    }
                    Wire::Anthropic => {
                        json!({"model":self.config.model_id,"system":system_prompt,"messages":messages,"tools":tools,"max_tokens":self.config.max_tokens,"temperature":self.config.temperature})
                    }
                    Wire::Ollama => {
                        json!({"model":self.config.model_id,"messages":messages,"tools":tools,"stream":false,"options":{"temperature":self.config.temperature,"num_predict":self.config.max_tokens}})
                    }
                    Wire::Chat => {
                        json!({"model":self.config.model_id,"messages":messages,"tools":tools,"tool_choice":"auto","max_tokens":self.config.max_tokens,"temperature":self.config.temperature})
                    }
                };
                if answer_only || explaining {
                    body.as_object_mut().unwrap().remove("tools");
                    body.as_object_mut().unwrap().remove("tool_choice");
                }
                if wire == Wire::Ollama && empty_retry {
                    body["options"]["num_predict"] = json!(self
                        .config
                        .max_tokens
                        .saturating_mul(4)
                        .clamp(1024, 4096)
                        .max(self.config.max_tokens));
                    if let Some(think) = &retry_think {
                        body["think"] = think.clone();
                    }
                }
                let reply = self.tool_request(body, wire, control, deadline)?;
                let truncated = match wire {
                    Wire::Chat => reply["choices"][0]["finish_reason"] == "length",
                    Wire::Anthropic => reply["stop_reason"] == "max_tokens",
                    Wire::Responses => reply["incomplete_details"]["reason"] == "max_output_tokens",
                    Wire::Ollama => reply["done_reason"] == "length",
                };
                let done_reason = reply["done_reason"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                let mut message = match wire {
                    Wire::Responses | Wire::Anthropic => reply,
                    Wire::Ollama => reply["message"].clone(),
                    Wire::Chat => reply["choices"][0]["message"].clone(),
                };
                if message.is_null() {
                    return Err(IronError::Agent(
                        "Backend returned no assistant message".into(),
                    ));
                }
                if self.config.backend_type == BackendType::IronWorks {
                    normalize_ironworks_call(&mut message, round)?;
                } else if wire == Wire::Ollama {
                    normalize_local_call(&mut message, round, "Ollama", true)?;
                }
                let mut calls = parse_calls(&message, wire)?;
                if wire == Wire::Ollama {
                    for call in &mut calls {
                        if let Some(definition) = catalog.iter().find(|tool| tool.name == call.name)
                        {
                            normalize_ollama_integer_args(&mut call.args, &definition.parameters);
                        }
                    }
                }
                if calls.is_empty() {
                    let answer = content(&message, wire);
                    if answer.trim().is_empty() {
                        if wire == Wire::Ollama && !empty_retry && round + 1 < MAX_ROUNDS {
                            empty_retry = true;
                            if message["thinking"]
                                .as_str()
                                .is_some_and(|s| !s.trim().is_empty())
                            {
                                // Ask the server rather than guessing which models accept false.
                                let metadata_deadline =
                                    deadline.min(Instant::now() + Duration::from_secs(2));
                                if let Ok(metadata) = self.request_at(
                                    json!({"model":self.config.model_id}),
                                    wire,
                                    "/api/show",
                                    control,
                                    metadata_deadline,
                                ) {
                                    retry_think = ollama_retry_thinking(&metadata);
                                }
                            }
                            control.report(
                                "Ollama",
                                "empty reply; retrying once with a bounded output budget",
                                Duration::ZERO,
                            );
                            // Keep all original messages and tool evidence. Never append the
                            // empty assistant message or use thinking as an answer/tool call.
                            continue;
                        }
                        let detail = if wire == Wire::Ollama {
                            format!("Ollama model '{}' returned no tool calls or final answer after one retry (done_reason: {}). Try another installed model or increase its output budget.", self.config.model_id, done_reason)
                        } else {
                            "Backend returned neither tools nor an answer".into()
                        };
                        return Err(IronError::Agent(detail));
                    }
                    if answer.lines().any(|line| {
                        let line = line.trim_start();
                        line.starts_with('{')
                            && line.contains("\"name\"")
                            && (line.contains("\"parameters\"") || line.contains("\"arguments\""))
                    }) {
                        return Err(IronError::Agent(
                            "Backend returned unrecognized tool-call syntax instead of a final answer; the text call was not executed.".into(),
                        ));
                    }
                    if used > 0 && successful_calls == 0 {
                        return Err(IronError::Agent("No monitoring tool succeeded; the backend answer was withheld because it has no tool evidence".into()));
                    }
                    if let Some(observed) = ram_leader {
                        return Ok(observed);
                    }
                    return Ok(if truncated {
                        format!("{answer}\n\n[Answer truncated: the backend reached its output token limit.]")
                    } else {
                        answer
                    });
                }
                if answer_only || explaining {
                    return Err(IronError::Agent("Backend requested another tool after the final-answer boundary; no additional tool was run".into()));
                }
                used += calls.len();
                if used > MAX_CALLS {
                    return Err(IronError::Agent("Agent exceeded its 16-call budget".into()));
                }
                if wire == Wire::Responses {
                    messages.extend(message["output"].as_array().cloned().unwrap_or_default());
                } else {
                    if wire == Wire::Ollama {
                        if let Some(fields) = message.as_object_mut() {
                            fields.remove("thinking");
                        }
                    }
                    if wire == Wire::Ollama && message["content"].is_null() {
                        message["content"] = json!("");
                    }
                    messages.push(if wire == Wire::Anthropic {
                        json!({"role":"assistant","content":message["content"]})
                    } else {
                        message
                    });
                }
                let mut anthropic_results = Vec::new();
                for call in calls {
                    if bytes >= TOTAL_BYTES {
                        return Err(IronError::Agent(
                            "Agent exceeded its tool-result memory budget".into(),
                        ));
                    }
                    let id = call.id.clone();
                    let name = call.name.clone();
                    // Keep only fixed-size fingerprints, not duplicate results.
                    // Two identical requests may be useful for rates or warmup;
                    // after that require an answer rather than an endless retry.
                    use std::hash::{Hash, Hasher};
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    call.name.hash(&mut hash);
                    call.args.to_string().hash(&mut hash);
                    let fingerprint = hash.finish();
                    if fingerprints.contains(&fingerprint) {
                        final_only = true;
                    }
                    fingerprints.push(fingerprint);
                    let output = tool_output(call, control, deadline, TOTAL_BYTES - bytes)?;
                    if let Ok(result) = serde_json::from_str::<Value>(&output) {
                        if result["success"] == true {
                            successful_calls += 1;
                        }
                        if render_ram_leader && name == "get_top_memory_processes" {
                            ram_leader = observed_ram_leader(&result);
                        }
                    }
                    bytes += output.len();
                    match wire {
                        Wire::Responses=>messages.push(json!({"type":"function_call_output","call_id":id,"output":output})),
                        Wire::Anthropic=>anthropic_results.push(json!({"type":"tool_result","tool_use_id":id,"content":output,"is_error":serde_json::from_str::<Value>(&output).is_ok_and(|v|v["success"]==false)})),
                        Wire::Ollama=>messages.push(json!({"role":"tool","tool_name":name,"content":output})),
                        Wire::Chat=>messages.push(json!({"role":"tool","tool_call_id":id,"content":output})),
                    }
                }
                if !anthropic_results.is_empty() {
                    messages.push(json!({"role":"user","content":anthropic_results}));
                }
                if serde_json::to_vec(&messages).map_or(true, |v| v.len() > 512 * 1024) {
                    return Err(IronError::Agent(
                        "Agent conversation exceeded 512 KiB".into(),
                    ));
                }
            }
            Err(IronError::Agent(
                "Agent exceeded six model rounds without a final answer".into(),
            ))
        }
    }
    #[cfg(feature = "remote-backends")]
    fn tool_request(
        &self,
        body: Value,
        wire: Wire,
        control: &RunControl,
        deadline: Instant,
    ) -> Result<Value> {
        let suffix = match wire {
            Wire::Responses => "/responses",
            Wire::Anthropic => "/messages",
            Wire::Ollama => "/api/chat",
            Wire::Chat => "/chat/completions",
        };
        self.request_at(body, wire, suffix, control, deadline)
    }

    #[cfg(feature = "remote-backends")]
    fn request_at(
        &self,
        body: Value,
        wire: Wire,
        suffix: &str,
        control: &RunControl,
        deadline: Instant,
    ) -> Result<Value> {
        use std::io::Read;
        let base = self
            .config
            .endpoint
            .as_ref()
            .ok_or_else(|| IronError::Configuration("No backend endpoint configured".into()))?;
        let url = format!("{}{suffix}", base.trim_end_matches('/'));
        let client = self.http_client.clone();
        let key = self.config.api_key.clone();
        work(
            &HTTP_WORK,
            "agent-http-worker",
            move || {
                let remaining = deadline.saturating_duration_since(Instant::now());
                let mut request = client.post(url).timeout(remaining).json(&body);
                if let Some(key) = key {
                    request = if wire == Wire::Anthropic {
                        request.header("x-api-key", key)
                    } else {
                        request.bearer_auth(key)
                    };
                }
                if wire == Wire::Anthropic {
                    request = request.header("anthropic-version", "2023-06-01");
                }
                let response = request.send().map_err(|e| {
                    IronError::Network(format!("Agent backend request failed: {e}"))
                })?;
                let status = response.status();
                let mut bytes = Vec::new();
                response
                    .take((RESPONSE_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)
                    .map_err(|e| IronError::Network(e.to_string()))?;
                if bytes.len() > RESPONSE_BYTES {
                    return Err(IronError::Agent("Backend response exceeded 512 KiB".into()));
                }
                if !status.is_success() {
                    return Err(IronError::Network(format!("Agent backend returned HTTP {status}; tools may be unsupported by the selected model")));
                }
                serde_json::from_slice(&bytes)
                    .map_err(|e| IronError::Parse(format!("Invalid agent backend response: {e}")))
            },
            control,
            deadline,
        )
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "remote-backends")]
    #[test]
    fn ollama_empty_replies_retry_once_without_losing_tool_evidence() {
        let _guard = crate::agent::offline_enforcement_tests::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (thinking, recovery, repeated_empty) in [
            (false, Value::Null, false),
            (true, json!(false), false),
            (true, json!("low"), false),
            (true, Value::Null, false),
            (false, Value::Null, true),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let server = std::thread::spawn(move || {
                let count = if thinking { 4 } else { 3 };
                let mut original = Value::Null;
                for step in 0..count {
                    let until = Instant::now() + Duration::from_secs(10);
                    let mut stream = loop {
                        match listener.accept() {
                            Ok((stream, _)) => break stream,
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                assert!(Instant::now() < until, "missing retry request");
                                std::thread::sleep(Duration::from_millis(5));
                            }
                            Err(e) => panic!("{e}"),
                        }
                    };
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let body = request(&mut stream);
                    assert_eq!(body["model"], "gemma4:e2b");
                    if thinking && step == 2 {
                        assert!(body.get("messages").is_none());
                        let metadata = if recovery.is_null() {
                            json!({})
                        } else {
                            json!({"thinking":{"values":[recovery],"default":true}})
                        };
                        respond(&mut stream, metadata);
                        continue;
                    }
                    assert!(body["tools"].as_array().is_some_and(|t| !t.is_empty()));
                    if step == 0 {
                        respond(
                            &mut stream,
                            json!({"message":{"role":"assistant","content":"","thinking":"private tool reasoning","tool_calls":[{"function":{"name":"describe_entities","arguments":{"search":"cpu.total","limit":1}}}]},"done":true}),
                        );
                    } else if step == 1 {
                        assert_eq!(body["options"]["num_predict"], 256);
                        assert_eq!(
                            body["messages"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .filter(|m| m["role"] == "tool")
                                .count(),
                            1
                        );
                        assert!(!body["messages"]
                            .to_string()
                            .contains("private tool reasoning"));
                        original = body;
                        respond(
                            &mut stream,
                            json!({"message":{"role":"assistant","content":"  ","thinking":if thinking {"private reasoning"} else {""}},"done":true,"done_reason":"length"}),
                        );
                    } else {
                        assert_eq!(body["messages"], original["messages"]);
                        assert_eq!(body["tools"], original["tools"]);
                        assert_eq!(body["options"]["num_predict"], 1024);
                        assert_eq!(
                            body.get("think"),
                            if recovery.is_null() {
                                None
                            } else {
                                Some(&recovery)
                            }
                        );
                        respond(
                            &mut stream,
                            json!({"message":{"role":"assistant","content":if repeated_empty {""} else {"CPU schema received."}},"done":true,"done_reason":"stop"}),
                        );
                    }
                }
            });
            let mut config = super::super::BackendConfig::ollama("gemma4:e2b");
            config.endpoint = Some(format!("http://{address}"));
            let mut agent =
                super::super::Agent::new(super::super::AgentConfig::with_backend(config)).unwrap();
            let result = agent.ask_with_control("Inspect CPU evidence", &RunControl::default());
            if repeated_empty {
                let error = result.unwrap_err().to_string();
                assert!(error.contains("gemma4:e2b"), "{error}");
                assert!(error.contains("after one retry"), "{error}");
                assert!(error.contains("done_reason: stop"), "{error}");
            } else {
                assert_eq!(result.unwrap().response, "CPU schema received.");
            }
            server.join().unwrap();
        }
    }
    #[test]
    fn conversational_context_keeps_complete_recent_turns_within_its_budget() {
        let mut history: Vec<_> = (0..10)
            .map(|n| ConversationTurn {
                user: format!("question {n}"),
                assistant: format!("answer {n}"),
            })
            .collect();
        let messages = conversation_messages(&history);
        assert_eq!(messages.len(), HISTORY_TURNS * 2);
        assert_eq!(messages[0]["content"], "question 4");
        assert_eq!(messages[11]["content"], "answer 9");
        assert!(messages.iter().all(|m| m["role"] != "system"));
        history.push(ConversationTurn {
            user: "What is that?".into(),
            assistant: "x".repeat(HISTORY_BYTES + 1),
        });
        assert!(conversation_messages(&history).is_empty());
        history.last_mut().unwrap().assistant = "x".repeat(HISTORY_BYTES - 20);
        assert_eq!(conversation_messages(&history).len(), 2);
        assert!(explanation_followup("What is that?", &history));
        assert!(!explanation_followup("What is that?", &[]));
        assert!(!explanation_followup(
            "Is that still using the most RAM?",
            &history
        ));
    }

    #[cfg(feature = "remote-backends")]
    #[test]
    #[ignore = "requires local Ollama with llama3.2:3b installed"]
    fn live_ollama_followup_resolves_memory_compression() {
        let config = super::super::AgentConfig::with_backend(super::super::BackendConfig::ollama(
            "llama3.2:3b",
        ));
        let mut agent = super::super::Agent::new(config).unwrap();
        let first = agent
            .ask_with_control(
                "Which process is using the most memory?",
                &RunControl::default(),
            )
            .unwrap();
        eprintln!("Live RAM answer: {}", first.response);
        let live_history = [ConversationTurn {
            user: first.query.clone(),
            assistant: first.response.clone(),
        }];
        let followup = agent
            .ask_with_history_and_control("What is that?", &live_history, &RunControl::default())
            .unwrap();
        eprintln!("Live referent explanation: {}", followup.response);
        assert!(!followup.response.contains("tool-call syntax"));
        let history = [ConversationTurn {
            user: "Which process is using the most memory?".into(),
            assistant: "\"Memory Compression\" is using the most resident RAM: 2482 MiB.".into(),
        }];
        for _ in 0..3 {
            let reply = agent
                .ask_with_history_and_control("What is that?", &history, &RunControl::default())
                .unwrap();
            eprintln!(
                "Follow-up ({} ms): {}",
                reply.inference_time_ms, reply.response
            );
            let text = reply.response.to_ascii_lowercase();
            assert!(text.contains("compress"), "{}", reply.response);
            assert!(!text.contains("using the most"), "{}", reply.response);
            assert!(text.contains("windows"), "{}", reply.response);
            assert!(
                text.contains("ram") || text.contains("pages"),
                "{}",
                reply.response
            );
            assert!(!text.contains("tool-call syntax"));
        }
    }

    #[cfg(feature = "remote-backends")]
    #[test]
    fn explanations_preserve_the_referent_without_advertising_measurement_tools() {
        let _guard = crate::agent::offline_enforcement_tests::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for wire in [Wire::Chat, Wire::Ollama, Wire::Anthropic, Wire::Responses] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let body = request(&mut stream);
                assert!(body.get("tools").is_none());
                assert!(body.get("tool_choice").is_none());
                let messages = if wire == Wire::Responses {
                    &body["input"]
                } else {
                    &body["messages"]
                };
                let first_user = if wire == Wire::Anthropic { 0 } else { 1 };
                assert_eq!(
                    messages[first_user]["content"],
                    "Which process uses the most memory?"
                );
                assert_eq!(messages[first_user + 1]["content"], "Memory Compression");
                assert_eq!(messages[first_user + 2]["content"], "What is that?");
                let text = "Memory Compression is managed by Windows.";
                let reply = match wire {
                    Wire::Chat => {
                        json!({"choices":[{"message":{"role":"assistant","content":text}}]})
                    }
                    Wire::Ollama => {
                        json!({"message":{"role":"assistant","content":text},"done":true})
                    }
                    Wire::Anthropic => json!({"content":[{"type":"text","text":text}]}),
                    Wire::Responses => {
                        json!({"output":[{"type":"message","content":[{"type":"output_text","text":text}]}]})
                    }
                };
                respond(&mut stream, reply);
            });
            let mut backend = match wire {
                Wire::Chat => super::super::BackendConfig::ironworks("fixture"),
                Wire::Ollama => super::super::BackendConfig::ollama("fixture"),
                Wire::Anthropic => {
                    super::super::BackendConfig::anthropic("fixture", Some("fixture".into()))
                }
                Wire::Responses => {
                    super::super::BackendConfig::openai("fixture", Some("fixture".into()))
                }
            };
            backend.endpoint = Some(format!(
                "http://{address}{}",
                if wire == Wire::Ollama { "" } else { "/v1" }
            ));
            let mut agent =
                super::super::Agent::new(super::super::AgentConfig::with_backend(backend)).unwrap();
            let result = agent
                .ask_with_history_and_control(
                    "What is that?",
                    &[ConversationTurn {
                        user: "Which process uses the most memory?".into(),
                        assistant: "Memory Compression".into(),
                    }],
                    &RunControl::default(),
                )
                .unwrap();
            assert!(result.response.contains("Memory Compression"));
            server.join().unwrap();
        }
    }
    #[test]
    fn ram_leader_rendering_uses_only_successful_ranked_observations() {
        let result = json!({"success":true,"data":[{
            "name":"worker\n.exe","memory_bytes":53_481 * 1024_u64 * 1024,
            "memory_mb":1,"memory_display":"invented text"
        }]});
        assert_eq!(
            observed_ram_leader(&result).unwrap(),
            "\"worker\\n.exe\" is using the most resident RAM: 53481 MiB."
        );
        for result in [
            json!({"success":false,"data":[{"name":"worker","memory_bytes":123}]}),
            json!({"success":true,"data":[]}),
            json!({"success":true,"data":[{"name":"worker"}]}),
            json!({"success":true,"data":[{"name":"worker","memory_bytes":null}]}),
        ] {
            assert!(observed_ram_leader(&result).is_none());
        }
        for question in [
            "What is using the most RAM?",
            "Which process is consuming the most memory right now?",
            "Which application is using the most RAM?",
        ] {
            assert!(simple_ram_ranking_question(question), "{question}");
        }
        for question in [
            "What is using the most RAM and why?",
            "How do I reduce memory usage?",
            "Which process used the most memory over the last hour?",
            "What is using the most CPU?",
            "Which process is using the most GPU memory?",
        ] {
            assert!(!simple_ram_ranking_question(question), "{question}");
        }
    }
    use super::*;

    #[test]
    fn ironworks_text_calls_are_exact_read_only_objects() {
        let mut message = json!({"role":"assistant","content":r#"{"name":"describe_entities","arguments":{"limit":1}}"#});
        normalize_ironworks_call(&mut message, 2).unwrap();
        let calls = parse_calls(&message, Wire::Chat).unwrap();
        assert_eq!(calls[0].name, "describe_entities");
        assert_eq!(calls[0].id, "ironworks-text-2");
        for text in [
            r#"{"name":"apply_profile_setting","arguments":{"confirm":true}}"#,
            r#"{"name":"unknown_function","arguments":{}}"#,
            r#"Example: {"name":"describe_entities","arguments":{}}"#,
            r#"{"name":"describe_entities","description":"example"}"#,
        ] {
            let mut message = json!({"content":text});
            normalize_ironworks_call(&mut message, 0).unwrap();
            assert!(parse_calls(&message, Wire::Chat).unwrap().is_empty());
        }
        for text in [
            r#"{"name":"describe_entities","arguments":{}} trailing"#,
            r#"{"name":"describe_entities","arguments":{},"extra":true}"#,
        ] {
            assert!(normalize_ironworks_call(&mut json!({"content":text}), 0).is_err());
        }
        let mut message = json!({"content":r#"{"name":"describe_entities","arguments":[]}"#});
        normalize_ironworks_call(&mut message, 0).unwrap();
        assert!(parse_calls(&message, Wire::Chat).is_err());
    }

    #[test]
    fn ollama_text_calls_keep_read_only_and_schema_boundaries() {
        for field in ["arguments", "parameters"] {
            let mut message = json!({"role":"assistant","content":json!({
                "name":"describe_entities",field:{"limit":1}
            }).to_string()});
            normalize_local_call(&mut message, 2, "Ollama", true).unwrap();
            let calls = parse_calls(&message, Wire::Ollama).unwrap();
            assert_eq!(calls[0].name, "describe_entities");
            assert_eq!(calls[0].args, json!({"limit":1}));
            assert_eq!(calls[0].id, "ollama-text-2");
        }
        for text in [
            r#"{"name":"apply_profile_setting","parameters":{"confirm":true}}"#,
            r#"{"name":"unknown_function","parameters":{}}"#,
            r#"Example: {"name":"describe_entities","parameters":{}}"#,
        ] {
            let mut message = json!({"content":text});
            normalize_local_call(&mut message, 0, "Ollama", true).unwrap();
            assert!(parse_calls(&message, Wire::Ollama).unwrap().is_empty());
        }
        for text in [
            r#"{"name":"describe_entities","parameters":{}} trailing"#,
            r#"{"name":"describe_entities","arguments":{},"parameters":{}}"#,
            r#"{"name":"describe_entities","parameters":{},"extra":true}"#,
        ] {
            assert!(normalize_local_call(&mut json!({"content":text}), 0, "Ollama", true).is_err());
        }
        let mut message =
            json!({"content":r#"{"name":"describe_entities","parameters":{"limit":"1"}}"#});
        normalize_local_call(&mut message, 0, "Ollama", true).unwrap();
        let calls = parse_calls(&message, Wire::Ollama).unwrap();
        let definition = definitions()
            .into_iter()
            .find(|tool| tool.name == calls[0].name)
            .unwrap();
        assert!(validate_args(&calls[0].args, &definition.parameters, 0).is_err());
    }

    #[test]
    fn ollama_integer_conversion_is_lossless_and_remains_schema_validated() {
        let schema = json!({"type":"object","properties":{
            "limit":{"type":"integer","minimum":1,"maximum":100},
            "name":{"type":"string"}
        }});
        let mut args = json!({"limit":"10","name":"20"});
        normalize_ollama_integer_args(&mut args, &schema);
        assert_eq!(args, json!({"limit":10,"name":"20"}));
        assert!(validate_args(&args, &schema, 0).is_ok());
        for text in [
            "01",
            " 1",
            "1.0",
            "1e2",
            "-1",
            "18446744073709551616",
            "101",
        ] {
            let mut args = json!({"limit":text});
            normalize_ollama_integer_args(&mut args, &schema);
            assert!(validate_args(&args, &schema, 0).is_err(), "Accepted {text}");
        }
    }

    #[cfg(feature = "remote-backends")]
    #[test]
    fn local_text_recovery_and_repeat_boundary_use_real_tool_results() {
        let _guard = crate::agent::offline_enforcement_tests::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for ollama in [false, true] {
            for (plain_text, refuses_final) in [(true, false), (false, false), (false, true)] {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                listener.set_nonblocking(true).unwrap();
                let address = listener.local_addr().unwrap();
                let server = std::thread::spawn(move || {
                    for round in 0..3 {
                        let until = Instant::now() + Duration::from_secs(5);
                        let mut stream = loop {
                            match listener.accept() {
                                Ok((s, _)) => break s,
                                Err(e)
                                    if e.kind() == std::io::ErrorKind::WouldBlock
                                        && Instant::now() < until =>
                                {
                                    std::thread::sleep(Duration::from_millis(5))
                                }
                                Err(e) => panic!("expected agent round {round}: {e}"),
                            }
                        };
                        let body = request(&mut stream);
                        if round > 0 {
                            let results: Vec<_> = body["messages"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .filter(|m| m["role"] == "tool")
                                .collect();
                            assert_eq!(results.len(), round);
                            let result: Value = serde_json::from_str(
                                results.last().unwrap()["content"].as_str().unwrap(),
                            )
                            .unwrap();
                            assert_eq!(result["success"], true);
                            assert_eq!(result["tool_name"], "describe_entities");
                            assert_eq!(result["data"]["schema_version"], "1.0");
                        }
                        if round == 2 {
                            assert!(body.get("tools").is_none());
                            assert!(body.get("tool_choice").is_none());
                        }
                        let message = if round == 2 && !refuses_final {
                            json!({"role":"assistant","content":"Final answer from observed schema version 1.0"})
                        } else if plain_text {
                            let field = if ollama { "parameters" } else { "arguments" };
                            json!({"role":"assistant","content":json!({"name":"describe_entities",field:{"search":"memory","limit":1}}).to_string()})
                        } else {
                            json!({"role":"assistant","content":null,"tool_calls":[{
                            "id":format!("call-{round}"),"type":"function","function":{
                                "name":"describe_entities","arguments":r#"{"search":"memory","limit":1}"#
                            }}]})
                        };
                        respond(
                            &mut stream,
                            if ollama {
                                json!({"message":message,"done":true})
                            } else {
                                json!({"choices":[{"message":message}]})
                            },
                        );
                    }
                });
                let mut backend = if ollama {
                    super::super::BackendConfig::ollama("fixture")
                } else {
                    super::super::BackendConfig::ironworks("fixture")
                };
                backend.endpoint = Some(if ollama {
                    format!("http://{address}")
                } else {
                    format!("http://{address}/v1")
                });
                let client = RemoteClient::new(backend).unwrap();
                let result = client.query_with_tools(
                    "Use evidence",
                    "Read schema",
                    Duration::from_secs(5),
                    &RunControl::default(),
                );
                if refuses_final {
                    assert!(result
                        .unwrap_err()
                        .to_string()
                        .contains("final-answer boundary"));
                } else {
                    assert!(result.unwrap().contains("Final answer"));
                }
                server.join().unwrap();
            }
        }
    }

    #[cfg(feature = "remote-backends")]
    fn request(stream: &mut std::net::TcpStream) -> Value {
        use std::io::Read;
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0; 1024];
        loop {
            let size = stream.read(&mut buffer).unwrap();
            assert!(size > 0);
            bytes.extend_from_slice(&buffer[..size]);
            assert!(bytes.len() < 256 * 1024);
            if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..end]);
                let length = header
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    return serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
                }
            }
        }
    }
    #[cfg(feature = "remote-backends")]
    fn respond(stream: &mut std::net::TcpStream, reply: Value) {
        use std::io::Write;
        let body = reply.to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    }
    #[cfg(feature = "remote-backends")]
    #[test]
    fn built_in_agent_executes_tools_and_returns_results_in_all_provider_formats() {
        let _guard = crate::agent::offline_enforcement_tests::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for wire in [Wire::Chat, Wire::Ollama, Wire::Anthropic, Wire::Responses] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let server = std::thread::spawn(move || {
                for round in 0..4 {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    let mut stream = loop {
                        match listener.accept() {
                            Ok((stream, _)) => break stream,
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                assert!(
                                    Instant::now() < deadline,
                                    "Agent did not make its expected request"
                                );
                                std::thread::sleep(Duration::from_millis(5));
                            }
                            Err(e) => panic!("{e}"),
                        }
                    };
                    let body = request(&mut stream);
                    let tools = body["tools"].as_array().unwrap();
                    let conversation = if wire == Wire::Responses {
                        &body["input"]
                    } else {
                        &body["messages"]
                    };
                    let first_user = if wire == Wire::Anthropic { 0 } else { 1 };
                    assert_eq!(
                        conversation[first_user]["content"],
                        "Which process used the most memory?"
                    );
                    assert_eq!(
                        conversation[first_user + 1]["content"],
                        "Memory Compression (historical reading)"
                    );
                    assert_eq!(
                        conversation[first_user + 2]["content"],
                        "Inspect current CPU evidence"
                    );
                    assert!(tools.iter().any(|t| t["name"] == "describe_entities"
                        || t["function"]["name"] == "describe_entities"));
                    assert!(!tools.iter().any(|t| t["name"] == "apply_profile_setting"
                        || t["function"]["name"] == "apply_profile_setting"));
                    let id = format!("call-{}", round / 2);
                    if round % 2 == 0 {
                        let function = json!({"name":"describe_entities","arguments":{"search":"cpu.total","limit":1}});
                        let reply = match wire {
                            Wire::Chat => {
                                json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":id,"type":"function","function":{"name":"describe_entities","arguments":r#"{"search":"cpu.total","limit":1}"#}}]}}]})
                            }
                            Wire::Ollama => {
                                let mut function = function;
                                function["arguments"]["limit"] = json!("1");
                                json!({"message":{"role":"assistant","content":"","tool_calls":[{"function":function}]},"done":true})
                            }
                            Wire::Anthropic => {
                                json!({"role":"assistant","content":[{"type":"tool_use","id":id,"name":"describe_entities","input":{"search":"cpu.total","limit":1}}],"stop_reason":"tool_use"})
                            }
                            Wire::Responses => {
                                json!({"output":[{"type":"reasoning","id":"rs_fixture","summary":[],"encrypted_content":"fixture"},{"type":"function_call","id":"fc_fixture","call_id":id,"name":"describe_entities","arguments":r#"{"search":"cpu.total","limit":1}"#}]})
                            }
                        };
                        respond(&mut stream, reply);
                    } else {
                        let rows = if wire == Wire::Responses {
                            assert_eq!(body["store"], false);
                            assert!(body["input"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|item| item["type"] == "reasoning"
                                    && item["encrypted_content"] == "fixture"));
                            &body["input"]
                        } else {
                            &body["messages"]
                        };
                        let last = rows.as_array().unwrap().last().unwrap();
                        let text = match wire {
                            Wire::Responses => {
                                assert_eq!(last["call_id"], id);
                                last["output"].as_str().unwrap()
                            }
                            Wire::Anthropic => {
                                assert_eq!(last["content"][0]["tool_use_id"], id);
                                last["content"][0]["content"].as_str().unwrap()
                            }
                            Wire::Chat => {
                                assert_eq!(last["tool_call_id"], id);
                                last["content"].as_str().unwrap()
                            }
                            Wire::Ollama => {
                                assert_eq!(last["tool_name"], "describe_entities");
                                last["content"].as_str().unwrap()
                            }
                        };
                        let result: Value = serde_json::from_str(text).unwrap();
                        assert_eq!(result["success"], true);
                        assert_eq!(result["data"]["entities"].as_array().unwrap().len(), 1);
                        assert!(result["data"]["entities"][0]["id"]
                            .as_str()
                            .unwrap()
                            .starts_with("cpu.total"));
                        let text = format!("Evidence received on run {}", round / 2);
                        let reply = match wire {
                            Wire::Chat => {
                                json!({"choices":[{"message":{"role":"assistant","content":text}}]})
                            }
                            Wire::Ollama => {
                                json!({"message":{"role":"assistant","content":text},"done":true})
                            }
                            Wire::Anthropic => {
                                json!({"content":[{"type":"text","text":text}],"stop_reason":"end_turn"})
                            }
                            Wire::Responses => {
                                json!({"output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":text}]}]})
                            }
                        };
                        respond(&mut stream, reply);
                    }
                }
            });
            let mut backend = super::super::BackendConfig::ironworks("fixture");
            backend.backend_type = match wire {
                Wire::Chat => BackendType::IronWorks,
                Wire::Ollama => BackendType::RemoteOllama,
                Wire::Anthropic => BackendType::RemoteAnthropic,
                Wire::Responses => BackendType::RemoteOpenAI,
            };
            backend.endpoint = Some(format!(
                "http://{address}{}",
                if wire == Wire::Ollama { "" } else { "/v1" }
            ));
            if matches!(wire, Wire::Responses | Wire::Anthropic) {
                backend.api_key = Some("fixture".into());
            }
            backend.timeout = Duration::from_secs(4);
            let mut agent =
                super::super::Agent::new(super::super::AgentConfig::with_backend(backend)).unwrap();
            let (tx, rx) = mpsc::sync_channel(16);
            let control = RunControl::with_progress(tx);
            for run in 0..2 {
                let answer = agent
                    .ask_with_history_and_control(
                        "Inspect current CPU evidence",
                        &[ConversationTurn {
                            user: "Which process used the most memory?".into(),
                            assistant: "Memory Compression (historical reading)".into(),
                        }],
                        &control,
                    )
                    .unwrap();
                assert_eq!(answer.response, format!("Evidence received on run {run}"));
                assert!(!answer.from_cache);
            }
            assert!(rx
                .try_iter()
                .any(|activity| activity.tool == "describe_entities"
                    && activity.phase == "completed"));
            server.join().unwrap();
        }
    }

    #[cfg(feature = "remote-backends")]
    #[test]
    fn token_limited_answer_is_explicitly_marked() {
        let _guard = crate::agent::offline_enforcement_tests::ENV_LOCK
            .lock()
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = request(&mut stream);
            respond(
                &mut stream,
                json!({"choices":[{"message":{"role":"assistant","content":"Partial observation"},"finish_reason":"length"}]}),
            );
        });
        let mut config = super::super::BackendConfig::ironworks("fixture");
        config.endpoint = Some(format!("http://{address}/v1"));
        let answer = RemoteClient::new(config)
            .unwrap()
            .query_with_tools(
                "Read observations",
                "Inspect CPU",
                Duration::from_secs(5),
                &RunControl::default(),
            )
            .unwrap();
        assert!(answer.starts_with("Partial observation"));
        assert!(answer.contains("Answer truncated"));
        server.join().unwrap();
    }

    #[cfg(feature = "remote-backends")]
    #[test]
    fn ollama_text_calls_are_not_successful_answers() {
        let _guard = crate::agent::offline_enforcement_tests::ENV_LOCK
            .lock()
            .unwrap();
        for text in [
            r#"{"name":"get_memory_status","parameters":{"}}"#,
            "Trying again.\n{\"name\":\"inspect_services\",\"parameters\":{\"limit\":2}}",
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let _ = request(&mut stream);
                respond(
                    &mut stream,
                    json!({"message":{"role":"assistant","content":text},"done":true}),
                );
            });
            let mut config = super::super::BackendConfig::ollama("fixture");
            config.endpoint = Some(format!("http://{address}"));
            let error = RemoteClient::new(config)
                .unwrap()
                .query_with_tools(
                    "Read observations",
                    "Inspect memory",
                    Duration::from_secs(5),
                    &RunControl::default(),
                )
                .unwrap_err();
            assert!(error.to_string().contains("tool-call syntax"), "{error}");
            server.join().unwrap();
        }
    }

    #[cfg(feature = "remote-backends")]
    #[test]
    fn ollama_cannot_pass_off_failed_tools_as_observed_ram_usage() {
        let _guard = crate::agent::offline_enforcement_tests::ENV_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for round in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let body = request(&mut stream);
                if round == 0 {
                    respond(
                        &mut stream,
                        json!({"message":{"role":"assistant","content":"","tool_calls":[{
                        "function":{"name":"get_top_memory_processes","arguments":{"count":"five"}}
                    }]},"done":true}),
                    );
                } else {
                    let messages = body["messages"].as_array().unwrap();
                    let result: Value =
                        serde_json::from_str(messages.last().unwrap()["content"].as_str().unwrap())
                            .unwrap();
                    assert_eq!(result["success"], false);
                    assert_eq!(
                        result["expected_parameters"]["properties"]["count"]["type"],
                        "integer"
                    );
                    respond(
                        &mut stream,
                        json!({"message":{"role":"assistant","content":"Chrome is using 8 GB of RAM"},"done":true}),
                    );
                }
            }
        });
        let mut config = super::super::BackendConfig::ollama("fixture");
        config.endpoint = Some(format!("http://{address}"));
        let error = RemoteClient::new(config)
            .unwrap()
            .query_with_tools(
                "Use observed data",
                "What is using the most RAM?",
                Duration::from_secs(5),
                &RunControl::default(),
            )
            .unwrap_err();
        assert!(error.to_string().contains("no tool evidence"), "{error}");
        server.join().unwrap();
    }

    #[cfg(feature = "remote-backends")]
    #[test]
    fn cancellation_and_short_deadlines_stop_waiting_for_a_stalled_backend() {
        for cancel in [true, false] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            // Keep the backend stalled until the client returns. A short sleep
            // races cancellation on loaded CI runners and can close the socket
            // before the cancellation thread gets scheduled.
            let (release, stalled) = std::sync::mpsc::channel();
            let server = std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(3);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(_) if Instant::now() < deadline => {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        Err(_) => return,
                    }
                };
                let _ = request(&mut stream);
                let _ = stalled.recv_timeout(Duration::from_secs(5));
            });
            let mut config = super::super::BackendConfig::ironworks("fixture");
            config.endpoint = Some(format!("http://{address}/v1"));
            let client = RemoteClient::new(config).unwrap();
            let control = RunControl::default();
            let cloned = control.clone();
            let trigger = cancel.then(|| {
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(50));
                    cloned.cancel();
                })
            });
            let started = Instant::now();
            let error = client
                .query_with_tools(
                    "Read observations",
                    "Inspect CPU",
                    if cancel {
                        Duration::from_secs(5)
                    } else {
                        Duration::from_millis(150)
                    },
                    &control,
                )
                .unwrap_err();
            let elapsed = started.elapsed();
            release.send(()).ok();
            // Allow scheduling slack while still proving the client does not
            // wait for the stalled backend or its five-second request budget.
            assert!(elapsed < Duration::from_secs(2), "elapsed: {elapsed:?}");
            assert!(
                error
                    .to_string()
                    .contains(if cancel { "cancelled" } else { "deadline" }),
                "{error}"
            );
            if let Some(trigger) = trigger {
                trigger.join().unwrap();
            }
            server.join().unwrap();
        }
    }

    #[test]
    fn schema_validation_rejects_unknown_fields_and_wrong_types() {
        let schema = json!({"type":"object","required":["limit"],"properties":{"limit":{"type":"integer","minimum":1,"maximum":100}}});
        assert!(validate_args(&json!({"limit":"100"}), &schema, 0).is_err());
        assert!(validate_args(&json!({"limit":101}), &schema, 0).is_err());
        assert!(validate_args(&json!({"limit":10,"execute":"anything"}), &schema, 0).is_err());
        assert!(validate_args(&json!({}), &schema, 0).is_err());
    }
    #[test]
    fn registry_excludes_writes_and_includes_diagnostics() {
        let names: Vec<_> = definitions().into_iter().map(|t| t.name).collect();
        assert!(names.len() >= 10);
        assert!(!names.iter().any(|n| n == "apply_profile_setting"));
        assert!(names.iter().any(|n| n == "query_os_events"));
    }
    #[test]
    fn excessive_call_batches_and_large_results_are_withheld() {
        let calls:Vec<_>=(0..17).map(|n|json!({"id":format!("call-{n}"),"function":{"name":"describe_entities","arguments":"{}"}})).collect();
        assert!(parse_calls(&json!({"tool_calls":calls}), Wire::Chat).is_err());
        let output = tool_output(
            Call {
                id: "x".into(),
                name: "describe_entities".into(),
                args: json!({"limit":1}),
            },
            &RunControl::default(),
            Instant::now() + Duration::from_secs(2),
            0,
        )
        .unwrap();
        let output: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(output["success"], false);
        assert!(output["withheld_bytes"].as_u64().unwrap() > 0);
    }
    #[test]
    fn cancelled_runs_dispatch_nothing() {
        let control = RunControl::default();
        control.cancel();
        assert!(control
            .check(Instant::now() + Duration::from_secs(1))
            .is_err());
    }
    #[test]
    fn malformed_and_repeated_calls_are_rejected() {
        assert!(parse_calls(&json!({"tool_calls":[{"id":"x","function":{"name":"describe_entities","arguments":"not JSON"}}]}),Wire::Chat).is_err());
        assert!(parse_calls(&json!({"tool_calls":[{"id":"x","function":{"name":"describe_entities","arguments":"{}"}},{"id":"x","function":{"name":"describe_entities","arguments":"{}"}}]}),Wire::Chat).is_err());
    }
    #[test]
    fn mutation_calls_are_refused_before_dispatch() {
        let result = tool_output(
            Call {
                id: "x".into(),
                name: "apply_profile_setting".into(),
                args: json!({"confirm":true}),
            },
            &RunControl::default(),
            Instant::now() + Duration::from_secs(1),
            RESULT_BYTES,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&result).unwrap()["success"],
            false
        );
    }
}

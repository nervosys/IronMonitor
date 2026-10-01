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

/// Some IronWorks checkpoints emit the complete call as JSON content instead
/// of the server's native call envelope. Accept only an exact, known read call;
/// never extract fragments from prose or repair malformed JSON.
fn normalize_ironworks_call(message: &mut Value, round: usize) -> Result<()> {
    if message["tool_calls"]
        .as_array()
        .is_some_and(|calls| !calls.is_empty())
    {
        return Ok(());
    }
    let text = message["content"].as_str().unwrap_or("").trim();
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        if text.starts_with("{\"name\"") && text.contains("\"arguments\"") {
            return Err(IronError::Agent(
                "IronWorks returned malformed tool-call text; no tool was run".into(),
            ));
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
    if !object.contains_key("arguments") {
        return Ok(());
    }
    if object.len() != 2 {
        return Err(IronError::Agent(
            "IronWorks returned an ambiguous tool-call object; no tool was run".into(),
        ));
    }
    // parse_calls and tool_output enforce the normal argument, schema and
    // read-only limits after normalization.
    *message = json!({"role":"assistant","content":null,"tool_calls":[{
        "id":format!("ironworks-text-{round}"),"type":"function",
        "function":{"name":name,"arguments":value["arguments"]}
    }]});
    Ok(())
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
        return Ok(json!({"success":false,"error":error}).to_string());
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
            let query = user_query.to_string();
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
            let _ = system_prompt;
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
            let tools:Vec<_>=definitions().into_iter().map(|t|match wire {
                Wire::Anthropic=>json!({"name":t.name,"description":t.description,"input_schema":t.parameters}),
                Wire::Responses=>json!({"type":"function","name":t.name,"description":t.description,"parameters":t.parameters,"strict":false}),
                _=>json!({"type":"function","function":{"name":t.name,"description":t.description,"parameters":t.parameters}}),
            }).collect();
            let mut messages = if wire == Wire::Anthropic {
                vec![json!({"role":"user","content":user_query})]
            } else {
                vec![
                    json!({"role":"system","content":system_prompt}),
                    json!({"role":"user","content":user_query}),
                ]
            };
            let mut used = 0;
            let mut bytes = 0;
            let mut fingerprints = Vec::new();
            let mut final_only = false;
            for round in 0..MAX_ROUNDS {
                control.check(deadline)?;
                let answer_only = self.config.backend_type == BackendType::IronWorks
                    && (final_only || round + 1 == MAX_ROUNDS);
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
                if answer_only {
                    body.as_object_mut().unwrap().remove("tools");
                    body.as_object_mut().unwrap().remove("tool_choice");
                }
                let reply = self.tool_request(body, wire, control, deadline)?;
                let truncated = match wire {
                    Wire::Chat => reply["choices"][0]["finish_reason"] == "length",
                    Wire::Anthropic => reply["stop_reason"] == "max_tokens",
                    Wire::Responses => reply["incomplete_details"]["reason"] == "max_output_tokens",
                    Wire::Ollama => reply["done_reason"] == "length",
                };
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
                }
                let calls = parse_calls(&message, wire)?;
                if calls.is_empty() {
                    let answer = content(&message, wire);
                    if answer.is_empty() {
                        return Err(IronError::Agent(
                            "Backend returned neither tools nor an answer".into(),
                        ));
                    }
                    if answer.lines().any(|line| {
                        let line = line.trim_start();
                        line.starts_with('{')
                            && line.contains("\"name\"")
                            && (line.contains("\"parameters\"") || line.contains("\"arguments\""))
                    }) {
                        return Err(IronError::Agent(
                            "Backend returned tool-call syntax instead of a final answer; no text-only call was executed. Use a model that supports this backend's native tool protocol.".into(),
                        ));
                    }
                    return Ok(if truncated {
                        format!("{answer}\n\n[Answer truncated: the backend reached its output token limit.]")
                    } else {
                        answer
                    });
                }
                if answer_only {
                    return Err(IronError::Agent("Backend requested another tool after the final-answer boundary; no additional tool was run".into()));
                }
                used += calls.len();
                if used > MAX_CALLS {
                    return Err(IronError::Agent("Agent exceeded its 16-call budget".into()));
                }
                if wire == Wire::Responses {
                    messages.extend(message["output"].as_array().cloned().unwrap_or_default());
                } else {
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
        use std::io::Read;
        let suffix = match wire {
            Wire::Responses => "/responses",
            Wire::Anthropic => "/messages",
            Wire::Ollama => "/api/chat",
            Wire::Chat => "/chat/completions",
        };
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

    #[cfg(feature = "remote-backends")]
    #[test]
    fn ironworks_text_recovery_and_repeat_boundary_use_real_tool_results() {
        let _guard = crate::agent::offline_enforcement_tests::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
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
                        json!({"role":"assistant","content":r#"{"name":"describe_entities","arguments":{"search":"memory","limit":1}}"#})
                    } else {
                        json!({"role":"assistant","content":null,"tool_calls":[{
                        "id":format!("call-{round}"),"type":"function","function":{
                            "name":"describe_entities","arguments":r#"{"search":"memory","limit":1}"#
                        }}]})
                    };
                    respond(&mut stream, json!({"choices":[{"message":message}]}));
                }
            });
            let mut backend = super::super::BackendConfig::ironworks("fixture");
            backend.endpoint = Some(format!("http://{address}/v1"));
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
                    .ask_with_control("Inspect current CPU evidence", &control)
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
    fn cancellation_and_short_deadlines_stop_waiting_for_a_stalled_backend() {
        for cancel in [true, false] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
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
                std::thread::sleep(Duration::from_millis(350));
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
                        Duration::from_secs(1)
                    } else {
                        Duration::from_millis(150)
                    },
                    &control,
                )
                .unwrap_err();
            assert!(started.elapsed() < Duration::from_millis(300));
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

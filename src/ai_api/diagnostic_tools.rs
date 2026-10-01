//! Read-only schema, service and operating-system event diagnostics.
use super::{AiDataApi, ToolCategory, ToolDefinition};
use crate::error::{IronError, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[cfg(any(windows, target_os = "linux"))]
use std::io::{Read, Write};
#[cfg(any(windows, target_os = "linux"))]
use std::process::{Command, Stdio};
#[cfg(any(windows, target_os = "linux"))]
use std::sync::mpsc;
use std::sync::OnceLock;
#[cfg(any(windows, target_os = "linux"))]
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

fn bad(message: impl Into<String>) -> IronError {
    IronError::Other(message.into())
}
fn limit() -> usize {
    50
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Describe {
    #[serde(default)]
    search: String,
    #[serde(default)]
    offset: usize,
    #[serde(default = "limit")]
    limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Services {
    #[serde(default)]
    names: Vec<String>,
    #[serde(default)]
    failed_only: bool,
    #[serde(default = "limit")]
    limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Events {
    #[serde(default = "system_log")]
    log: String,
    start_ms: u64,
    end_ms: u64,
    #[serde(default)]
    before_record_id: Option<u64>,
    #[serde(default = "limit")]
    limit: usize,
}
fn system_log() -> String {
    "System".into()
}

impl AiDataApi {
    pub(super) fn tool_describe_entities(&mut self, params: Value) -> Result<Value> {
        let p: Describe = serde_json::from_value(params).map_err(|e| bad(e.to_string()))?;
        if p.search.len() > 128 || !(1..=100).contains(&p.limit) || p.offset > 10000 {
            return Err(bad("search <=128 bytes, limit 1..=100, offset <=10000"));
        }
        static SCHEMA: OnceLock<crate::ontology::Ontology> = OnceLock::new();
        let schema = SCHEMA.get_or_init(crate::ontology::Ontology::build);
        let matches = schema.search(&p.search);
        let entities: Vec<_> = matches.iter().skip(p.offset).take(p.limit).collect();
        Ok(
            json!({"schema_version":schema.version,"entities":entities,"total_matches":matches.len(),
            "next_offset":(p.offset+entities.len()<matches.len()).then_some(p.offset+entities.len()),
            "note":"Schema discovery reads no hardware. Expand templates against live observations; declared entities may be unavailable to the bounded collector."}),
        )
    }
    pub(super) fn tool_inspect_services(&mut self, params: Value) -> Result<Value> {
        let p: Services = serde_json::from_value(params).map_err(|e| bad(e.to_string()))?;
        if !(1..=100).contains(&p.limit)
            || p.names.len() > 16
            || p.names.iter().any(|n| n.is_empty() || n.len() > 128)
        {
            return Err(bad(
                "limit 1..=100; at most 16 service names of 1..=128 bytes",
            ));
        }
        #[cfg(windows)]
        if p.failed_only {
            return Ok(
                json!({"provenance":"unavailable","status":"unavailable","reason":"Windows service status enumeration does not expose a distinct failed state. Inspect stopped services and query_os_events for recorded failures."}),
            );
        }
        #[cfg(windows)]
        let raw = command_json("powershell.exe", &["-NoProfile".into(), "-NonInteractive".into(), "-Command".into(),
            "$ErrorActionPreference='Stop'; $p=[Console]::In.ReadToEnd()|ConvertFrom-Json; $all=@(Get-Service); $rows=@($all|Where-Object {($p.names.Count -eq 0 -or $p.names -ccontains $_.Name) -and (-not $p.failed_only -or $_.Status.ToString() -eq 'Failed')}|Sort-Object Name); $selected=@($rows|Select-Object -First $p.limit|ForEach-Object { @{name=$_.Name;display_name=$_.DisplayName;status=$_.Status.ToString();startup_type=$_.StartType.ToString()} }); @{services=$selected;matched=$rows.Count;enumerated=$all.Count}|ConvertTo-Json -Depth 5 -Compress".into()],
            Some(json!({"names":p.names,"failed_only":p.failed_only,"limit":p.limit})))?;
        #[cfg(target_os = "linux")]
        let raw = {
            let rows = command_json(
                "systemctl",
                &[
                    "list-units".into(),
                    "--all".into(),
                    "--type=service".into(),
                    "--output=json".into(),
                    "--no-pager".into(),
                ],
                None,
            )?;
            if rows["status"] == "unavailable" {
                return Ok(rows);
            }
            let all = rows
                .as_array()
                .ok_or_else(|| bad("Service provider did not return an array"))?;
            let matched: Vec<_> = all
                .iter()
                .filter(|row| {
                    p.names.is_empty() || p.names.iter().any(|n| row["unit"].as_str() == Some(n))
                })
                .filter(|row| !p.failed_only || row["active"] == "failed")
                .collect();
            json!({"services":matched.iter().take(p.limit).collect::<Vec<_>>(),"matched":matched.len(),"enumerated":all.len()})
        };
        #[cfg(not(any(windows, target_os = "linux")))]
        let raw = json!({"status":"unavailable","reason":"Bounded service diagnostics are implemented for Windows and systemd hosts"});
        if raw["status"] == "unavailable" {
            return Ok(raw);
        }
        Ok(
            json!({"provenance":"measured","sampled_at_ms":now_ms(),"services":raw["services"],
            "matched":raw["matched"],"enumerated":raw["enumerated"],"truncated":raw["matched"].as_u64().unwrap_or(0)>p.limit as u64,
            "requested_names":p.names,
            "note":"Read-only status enumeration. Windows reports stopped services separately from failed services; a stopped service is not assumed failed. Omission from a capped or filtered result does not prove absence."}),
        )
    }
    pub(super) fn tool_query_os_events(&mut self, params: Value) -> Result<Value> {
        let p: Events = serde_json::from_value(params).map_err(|e| bad(e.to_string()))?;
        if !matches!(p.log.as_str(), "System" | "Application")
            || p.end_ms < p.start_ms
            || p.end_ms - p.start_ms > 86_400_000
            || p.end_ms > now_ms().saturating_add(1000)
            || !(1..=100).contains(&p.limit)
            || p.before_record_id == Some(0)
        {
            return Err(bad("System/Application log, ordered time range <=24 hours ending no later than now, limit 1..=100, positive record cursor"));
        }
        #[cfg(windows)]
        let raw = command_json("powershell.exe", &["-NoProfile".into(),"-NonInteractive".into(),"-Command".into(),
            r#"$ErrorActionPreference='Stop'; $p=[Console]::In.ReadToEnd()|ConvertFrom-Json; $start=[DateTimeOffset]::FromUnixTimeMilliseconds($p.start_ms).UtcDateTime.ToString('yyyy-MM-ddTHH:mm:ss.fffZ'); $end=[DateTimeOffset]::FromUnixTimeMilliseconds($p.end_ms).UtcDateTime.ToString('yyyy-MM-ddTHH:mm:ss.fffZ'); $query="*[System[(Level=1 or Level=2 or Level=3) and TimeCreated[@SystemTime>='$start' and @SystemTime<='$end']"; if ($null -ne $p.before_record_id) {$query+=" and EventRecordID<$($p.before_record_id)"}; $query+=']]'; try {$events=@(Get-WinEvent -LogName $p.log -FilterXPath $query -MaxEvents ($p.limit+1) -ErrorAction Stop)} catch {if ($_.FullyQualifiedErrorId -like 'NoMatchingEventsFound*') {$events=@()} else {throw}}; $rows=@($events|Select-Object -First $p.limit|ForEach-Object {$message=[string]$_.Message; if ($message.Length -gt 1024) {$message=$message.Substring(0,1024)}; @{record_id=$_.RecordId;event_id=$_.Id;provider=$_.ProviderName;level=$_.Level;timestamp_ms=([DateTimeOffset]$_.TimeCreated).ToUnixTimeMilliseconds();message=$message;message_truncated=([string]$_.Message).Length -gt 1024}}); @{events=$rows;has_more=($events.Count -gt $p.limit)}|ConvertTo-Json -Depth 5 -Compress"#.into()],
            Some(json!({"log":p.log,"start_ms":p.start_ms,"end_ms":p.end_ms,"before_record_id":p.before_record_id,"limit":p.limit})))?;
        #[cfg(target_os = "linux")]
        let raw = {
            if p.before_record_id.is_some() {
                return Err(bad("Record-ID pagination applies to Windows logs; narrow the time range on journal hosts"));
            }
            match bounded_command(
                "journalctl",
                &[
                    "--output=json".into(),
                    "--reverse".into(),
                    "--no-pager".into(),
                    "--priority=0..4".into(),
                    format!("--since=@{}", p.start_ms / 1000),
                    format!("--until=@{}", p.end_ms / 1000),
                    format!("--lines={}", p.limit + 1),
                ],
                None,
            ) {
                Ok(bytes) => {
                    let rows: Vec<Value> = bytes
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(serde_json::from_str)
                        .collect::<std::result::Result<_, _>>()
                        .map_err(|e| bad(e.to_string()))?;
                    json!({"events":rows.iter().take(p.limit).map(|r| json!({"timestamp_us":r["__REALTIME_TIMESTAMP"],"provider":r["SYSLOG_IDENTIFIER"],"message":r["MESSAGE"].as_str().map(|m|m.chars().take(1024).collect::<String>()),"message_truncated":r["MESSAGE"].as_str().is_some_and(|m|m.chars().count()>1024),"cursor":r["__CURSOR"],"priority":r["PRIORITY"]})).collect::<Vec<_>>(),"has_more":rows.len()>p.limit})
                }
                Err(error) => json!({"status":"unavailable","reason":error.to_string()}),
            }
        };
        #[cfg(not(any(windows, target_os = "linux")))]
        let raw = json!({"status":"unavailable","reason":"Bounded operating-system log diagnostics are implemented for Windows and journald hosts"});
        if raw["status"] == "unavailable" {
            return Ok(raw);
        }
        let next = raw["events"]
            .as_array()
            .and_then(|e| e.last())
            .and_then(|e| e.get("record_id"))
            .cloned();
        Ok(
            json!({"provenance":"measured","queried_at_ms":now_ms(),"log":p.log,"events":raw["events"],"has_more":raw["has_more"],"next_before_record_id":next,
            "scope":if cfg!(windows) {"Requested Windows log: critical, error and warning records"} else {"Local journal: emergency through warning records; System/Application is a Windows-only distinction"},
            "note":"Bounded read-only log query. Record cursors must be reused with the same log and time range; journal pagination uses a narrower time range."}),
        )
    }
}

#[cfg(any(windows, target_os = "linux"))]
fn command_json(program: &str, args: &[String], input: Option<Value>) -> Result<Value> {
    match bounded_command(program, args, input) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| bad(format!("Diagnostic provider returned invalid JSON: {e}"))),
        Err(error) => Ok(json!({"status":"unavailable","reason":error.to_string()})),
    }
}
// Drain stdout concurrently: a full pipe must not stall the child. Output and
// lifetime are capped; timeout and overflow kill and reap the direct child.
#[cfg(any(windows, target_os = "linux"))]
fn bounded_command(program: &str, args: &[String], input: Option<Value>) -> Result<String> {
    const CAP: usize = 256 * 1024;
    let mut command = Command::new(program);
    let mut args = args.to_vec();
    if program == "powershell.exe" {
        if let Some(script) = args.last_mut() {
            *script=format!("[Console]::InputEncoding=[Text.UTF8Encoding]::new($false); [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); {script}");
        }
    }
    command
        .args(&args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| bad(format!("Diagnostic provider could not start: {e}")))?;
    if let Some(input) = input {
        let bytes = serde_json::to_vec(&input).map_err(|e| bad(e.to_string()))?;
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(&bytes) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(bad(e.to_string()));
            }
        }
    }
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| bad("Diagnostic provider stdout unavailable"))?;
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take((CAP + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = tx.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if let Ok(result) = rx.try_recv() {
            let bytes = match result {
                Ok(bytes) => bytes,
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(bad(error.to_string()));
                }
            };
            if bytes.len() > CAP {
                let _ = child.kill();
                let _ = child.wait();
                return Err(bad("Diagnostic provider exceeded the 256 KiB output limit"));
            }
            while Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        return if status.success() {
                            String::from_utf8(bytes).map_err(|e| bad(e.to_string()))
                        } else {
                            Err(bad(format!("Diagnostic provider exited with {status}; query may be unsupported or access denied")))
                        }
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                    Err(e) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(bad(e.to_string()));
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(bad("Diagnostic provider exceeded its four-second deadline"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn definitions() -> Vec<ToolDefinition> {
    [
        ("describe_entities","Discover ontology IDs/templates, units, provenance, ranges and descriptions without touching hardware. Paginate with offset.",json!({"search":{"type":"string","maxLength":128},"offset":{"type":"integer","minimum":0,"maximum":10000},"limit":{"type":"integer","minimum":1,"maximum":100,"default":50}}),vec![]),
        ("list_connections","Read the collector's last socket table with its original sampling time. Filter PID, local port, remote IP or listening sockets; capped results announce truncation.",json!({"pid":{"type":"integer","minimum":1,"maximum":u32::MAX},"local_port":{"type":"integer","minimum":0,"maximum":65535},"remote_ip":{"type":"string","maxLength":45},"listening_only":{"type":"boolean"},"limit":{"type":"integer","minimum":1,"maximum":100,"default":50},"max_age_ms":{"type":"integer","minimum":1,"maximum":60000,"default":5000}}),vec![]),
        ("inspect_services","Read bounded service status, optionally by exact names or failed status. No service control. Unsupported providers or failed queries are unavailable.",json!({"names":{"type":"array","maxItems":16,"items":{"type":"string","minLength":1,"maxLength":128}},"failed_only":{"type":"boolean"},"limit":{"type":"integer","minimum":1,"maximum":100,"default":50}}),vec![]),
        ("query_os_events","Read critical/error/warning OS events in a bounded time window. Windows System/Application logs support before_record_id pagination; Linux reads journald. Maximum 24-hour window and 100 records.",json!({"log":{"type":"string","enum":["System","Application"],"default":"System"},"start_ms":{"type":"integer","minimum":0},"end_ms":{"type":"integer","minimum":0},"before_record_id":{"type":"integer","minimum":1},"limit":{"type":"integer","minimum":1,"maximum":100,"default":50}}),vec!["start_ms","end_ms"]),
    ].into_iter().map(|(name,description,properties,required)|ToolDefinition{name:name.into(),description:description.into(),parameters:json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}),category:ToolCategory::System,example:None}).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn provider_input_is_utf8_data_and_output_is_bounded() {
        let args=vec!["-NoProfile".into(),"-NonInteractive".into(),"-Command".into(),"$p=[Console]::In.ReadToEnd()|ConvertFrom-Json; @{text=$p.text}|ConvertTo-Json -Compress".into()];
        let text = "日次 $(this remains data)";
        let result = command_json("powershell.exe", &args, Some(json!({"text":text}))).unwrap();
        assert_eq!(result["text"], text);
        let args = vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            "[Console]::Write(('x'*270000))".into(),
        ];
        assert!(bounded_command("powershell.exe", &args, None)
            .unwrap_err()
            .to_string()
            .contains("output limit"));
    }
    #[cfg(windows)]
    #[test]
    fn stalled_diagnostic_process_is_killed_on_deadline() {
        let started = Instant::now();
        let args = vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            "[Threading.Thread]::Sleep(6000)".into(),
        ];
        assert!(bounded_command("powershell.exe", &args, None)
            .unwrap_err()
            .to_string()
            .contains("deadline"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
    #[test]
    fn discovery_is_versioned_and_paginated() {
        let mut api = AiDataApi::with_components(None, None, None);
        let a = api
            .tool_describe_entities(json!({"search":"gpu","limit":2}))
            .unwrap();
        assert_eq!(a["entities"].as_array().unwrap().len(), 2);
        assert_eq!(a["next_offset"], 2);
        let b = api
            .tool_describe_entities(json!({"search":"gpu","limit":2,"offset":2}))
            .unwrap();
        assert_ne!(a["entities"][0]["id"], b["entities"][0]["id"]);
        assert_eq!(a["schema_version"], crate::ontology::ONTOLOGY_VERSION);
    }
    #[test]
    fn diagnostics_reject_unbounded_or_unknown_arguments() {
        let mut api = AiDataApi::with_components(None, None, None);
        assert!(api.tool_inspect_services(json!({"limit":1000})).is_err());
        assert!(api
            .tool_query_os_events(json!({"log":"Security","start_ms":0,"end_ms":1}))
            .is_err());
        assert!(api
            .tool_query_os_events(json!({"start_ms":0,"end_ms":86_400_001}))
            .is_err());
        assert!(api
            .tool_describe_entities(json!({"execute":"anything"}))
            .is_err());
    }
}

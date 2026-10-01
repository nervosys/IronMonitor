//! Agent observability tools run off the GUI thread through the real registry.
use super::theme::{self, CyberColors};
use crate::ai_api::AiDataApi;
use eframe::egui::{self, RichText};
use std::sync::mpsc::{self, Receiver};

const TOOLS: [(&str, &str); 10] = [
    ("get_observation_snapshot", "Live observations"),
    ("get_collector_health", "Collector health"),
    ("query_metric_history", "Metric history"),
    ("query_events", "Observation events"),
    ("inspect_process", "Inspect process"),
    ("check_endpoint", "Endpoint check"),
    ("describe_entities", "Entity discovery"),
    ("list_connections", "Socket connections"),
    ("inspect_services", "Service status"),
    ("query_os_events", "OS events"),
];

pub(super) struct AgentToolsPanel {
    selected: usize,
    ids: String,
    pid: u32,
    window_secs: u64,
    url: String,
    search: String,
    service_names: String,
    log: String,
    listening_only: bool,
    pending: Option<Receiver<String>>,
    response: Option<String>,
}

impl Default for AgentToolsPanel {
    fn default() -> Self {
        Self {
            selected: 0,
            ids: "cpu.total.utilization,memory.used,memory.total".into(),
            pid: std::process::id(),
            window_secs: 30,
            url: "http://127.0.0.1:11434/".into(),
            search: "cpu".into(),
            service_names: String::new(),
            log: "System".into(),
            listening_only: false,
            pending: None,
            response: None,
        }
    }
}

impl AgentToolsPanel {
    pub(super) fn draw(&mut self, ui: &mut egui::Ui) {
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(response) => {
                    self.response = Some(response);
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.response = Some("Tool worker exited before returning a result".into());
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        ui.label(RichText::new("Shared observations, bounded history, events, and endpoint diagnostics for AI agents.")
            .color(theme::color(ui.ctx(), CyberColors::TEXT_SECONDARY)));
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("agent_observability_tool")
                .selected_text(TOOLS[self.selected].1).show_ui(ui, |ui| {
                    for (index, (_, label)) in TOOLS.iter().enumerate() {
                        ui.selectable_value(&mut self.selected, index, *label);
                    }
                });
            match self.selected {
                0 | 2 => {
                    ui.label("Entity IDs:");
                    ui.add(egui::TextEdit::singleline(&mut self.ids).desired_width(280.0).char_limit(2048));
                }
                4 => {
                    ui.label("PID:");
                    ui.add(egui::DragValue::new(&mut self.pid).range(1..=u32::MAX));
                }
                5 => {
                    ui.label("URL:");
                    ui.add(egui::TextEdit::singleline(&mut self.url).desired_width(280.0).char_limit(2048));
                }
                6 => {ui.label("Search:");ui.add(egui::TextEdit::singleline(&mut self.search).desired_width(220.0).char_limit(128));}
                7 => {ui.checkbox(&mut self.listening_only,"Listening only");}
                8 => {ui.label("Service names (optional):");ui.add(egui::TextEdit::singleline(&mut self.service_names).desired_width(220.0).char_limit(2048));}
                9 => {egui::ComboBox::from_id_salt("agent_os_log").selected_text(&self.log).show_ui(ui,|ui|{for log in ["System","Application"]{ui.selectable_value(&mut self.log,log.into(),log);}});}
                _ => {}
            }
            if matches!(self.selected, 2 | 4) {
                self.window_secs=self.window_secs.min(30);
                ui.add(egui::DragValue::new(&mut self.window_secs).range(1..=30).suffix(" seconds"));
            }
            if self.selected==9 {ui.add(egui::DragValue::new(&mut self.window_secs).range(1..=86400).suffix(" seconds"));}
            if ui.add_enabled(self.pending.is_none(), egui::Button::new("Run tool")).clicked() {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default().as_millis() as u64;
                let ids: Vec<_> = self.ids.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
                let params = match self.selected {
                    0 => serde_json::json!({"ids":ids,"wait_ms":1000}),
                    2 => serde_json::json!({"ids":ids,"start_ms":now.saturating_sub(self.window_secs*1000),"end_ms":now,"max_points":60}),
                    4 => serde_json::json!({"pid":self.pid,"window_secs":self.window_secs}),
                    5 => serde_json::json!({"url":self.url,"timeout_ms":3000}),
                    6 => serde_json::json!({"search":self.search,"limit":20}),
                    7 => serde_json::json!({"listening_only":self.listening_only,"limit":50}),
                    8 => serde_json::json!({"names":self.service_names.split(',').map(str::trim).filter(|s|!s.is_empty()).collect::<Vec<_>>(),"limit":50}),
                    9 => serde_json::json!({"log":self.log,"start_ms":now.saturating_sub(self.window_secs*1000),"end_ms":now,"limit":50}),
                    _ => serde_json::json!({}),
                };
                let name = TOOLS[self.selected].0;
                let ctx = ui.ctx().clone();
                let (tx, rx) = mpsc::sync_channel(1);
                match std::thread::Builder::new().name("gui-agent-tool".into()).spawn(move || {
                    let mut api = AiDataApi::with_components(None, None, None);
                    let result = api.call_tool(name, params);
                    let text = match result {
                        Ok(result) => serde_json::to_string_pretty(&result).unwrap_or_else(|e| e.to_string()),
                        Err(error) => error.to_string(),
                    };
                    // Keep the visible response bounded; complete results remain
                    // available through the agent API instead of a giant UI string.
                    let text = if text.len() > 16_384 {
                        let mut end = 16_384;
                        while !text.is_char_boundary(end) { end -= 1; }
                        format!("{}\n[Display truncated at 16,384 bytes; use the agent API for the full result]", &text[..end])
                    } else { text };
                    let _ = tx.send(text);
                    ctx.request_repaint();
                }) {
                    Ok(_) => { self.pending = Some(rx); self.response = None; }
                    Err(error) => self.response = Some(format!("Could not start tool worker: {error}")),
                }
            }
            if self.pending.is_some() { ui.spinner(); ui.label("Running…"); }
        });
        if let Some(response) = &self.response {
            ui.horizontal(|ui| {
                if ui.button("Copy agent result").clicked() {
                    ui.ctx().copy_text(response.clone());
                }
                ui.label(
                    RichText::new("Structured result with provenance and collection timestamps")
                        .small()
                        .color(theme::color(ui.ctx(), CyberColors::TEXT_MUTED)),
                );
            });
            egui::ScrollArea::vertical()
                .id_salt("agent_tool_response")
                .max_height(220.0)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(response).monospace()).wrap());
                });
        }
    }
}

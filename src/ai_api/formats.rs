//! AI Agent Format Exporters
//!
//! Export tools and ontology in formats understood by various AI agents.

use super::tools::get_all_tool_definitions;
use super::{ToolCategory, ToolDefinition};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Export format for different AI agent systems
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    // These name the provider and its wire format, deliberately not the model
    // generations they were current for. A comment listing model names is a comment
    // that will be wrong by the next release, and the format is what actually
    // differs between these variants.
    /// OpenAI function-calling format.
    OpenAI,
    /// Anthropic tool-use format.
    Anthropic,
    /// Google Gemini function-declaration format.
    Gemini,
    /// xAI — OpenAI-compatible format.
    Grok,
    /// Meta Llama, via any host that serves it — OpenAI-compatible format.
    Llama,
    /// Mistral — OpenAI-compatible format.
    Mistral,
    /// DeepSeek — OpenAI-compatible format.
    DeepSeek,
    /// JSON-LD for semantic web discovery
    JsonLd,
    /// Model Context Protocol (Claude Desktop, etc.)
    Mcp,
    /// Simple JSON manifest
    SimpleJson,
}

/// Complete AI agent manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    pub version: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub tools: Vec<ToolDefinition>,
}

impl AgentManifest {
    pub fn new() -> Self {
        Self {
            version: "1.0.0".to_string(),
            name: "IronMonitor".to_string(),
            description: "Comprehensive hardware monitoring for AI agents.".to_string(),
            capabilities: vec![
                "hardware_monitoring".to_string(),
                "gpu_monitoring".to_string(),
                "cpu_monitoring".to_string(),
                "memory_monitoring".to_string(),
                "process_monitoring".to_string(),
            ],
            tools: get_all_tool_definitions(),
        }
    }

    pub fn export(&self, format: ExportFormat) -> Value {
        match format {
            ExportFormat::OpenAI => self.to_openai(),
            ExportFormat::Anthropic => self.to_anthropic(),
            ExportFormat::Gemini => self.to_gemini(),
            ExportFormat::Grok => self.to_grok(),
            ExportFormat::Llama => self.to_llama(),
            ExportFormat::Mistral => self.to_mistral(),
            ExportFormat::DeepSeek => self.to_deepseek(),
            ExportFormat::JsonLd => self.to_json_ld(),
            ExportFormat::Mcp => self.to_mcp(),
            ExportFormat::SimpleJson => serde_json::to_value(self).unwrap_or(json!({})),
        }
    }

    /// Where to ask a provider what it currently serves.
    ///
    /// This replaced a per-provider `supported_models` array. That array was a
    /// frozen enumeration of model ids, and it was wrong twice over: it went stale
    /// the moment a provider shipped anything (it still advertised `gpt-4o` and
    /// `claude-3-opus-20240229` long after both were superseded), and it was never
    /// true in the first place — this manifest describes *tools*, so any model that
    /// can call tools in the provider's format can consume it. There was nothing to
    /// "support".
    ///
    /// Naming the listing endpoint instead is strictly better for the agent reading
    /// this: it answers the question the array was pretending to answer, it answers
    /// it about the caller's own account rather than about the day this file was
    /// written, and it cannot rot.
    fn model_discovery(endpoint: &str) -> Value {
        json!({
            "endpoint": endpoint,
            "method": "GET",
            "note": "Ask this endpoint for the models this account can reach. \
                     Any model that supports tool calling in this format works; \
                     the `model` field above is only a starting suggestion.",
        })
    }

    fn to_openai(&self) -> Value {
        let functions: Vec<Value> = self
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            })
            .collect();
        json!({
            "model": crate::agent::DEFAULT_OPENAI_MODEL,
            "tools": functions,
            "tool_choice": "auto",
            "model_discovery": Self::model_discovery("https://api.openai.com/v1/models"),
        })
    }

    fn to_anthropic(&self) -> Value {
        let tools: Vec<Value> = self
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.parameters,
                })
            })
            .collect();
        json!({
            "model": crate::agent::DEFAULT_ANTHROPIC_MODEL,
            "tools": tools,
            "model_discovery": Self::model_discovery("https://api.anthropic.com/v1/models"),
        })
    }

    fn to_gemini(&self) -> Value {
        let function_declarations: Vec<Value> = self
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                })
            })
            .collect();
        json!({
            "tools": [{ "function_declarations": function_declarations }],
            "model_discovery": Self::model_discovery(
                "https://generativelanguage.googleapis.com/v1beta/models",
            ),
        })
    }

    fn to_grok(&self) -> Value {
        // xAI Grok uses OpenAI-compatible format
        let functions: Vec<Value> = self
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            })
            .collect();
        json!({
            "tools": functions,
            "tool_choice": "auto",
            "model_discovery": Self::model_discovery("https://api.x.ai/v1/models"),
        })
    }

    fn to_llama(&self) -> Value {
        // Meta Llama via various providers (Together, Fireworks, etc.) - OpenAI-compatible
        let functions: Vec<Value> = self
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            })
            .collect();
        json!({
            "tools": functions,
            "tool_choice": "auto",
            // Llama is served by many hosts, each with its own catalogue and its own
            // naming, so there is no one endpoint to name here.
            "model_discovery": {
                "note": "Llama is served by third parties (Together, Fireworks, Groq, \
                         and others). Ask your chosen provider's OpenAI-compatible \
                         `/v1/models` endpoint for the models and exact ids it serves.",
            },
        })
    }

    fn to_mistral(&self) -> Value {
        // Mistral AI - OpenAI-compatible format
        let functions: Vec<Value> = self
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            })
            .collect();
        json!({
            // A `-latest` alias tracks the current release, so unlike a pinned id
            // this default does not go stale on its own.
            "model": "mistral-large-latest",
            "tools": functions,
            "tool_choice": "auto",
            "model_discovery": Self::model_discovery("https://api.mistral.ai/v1/models"),
        })
    }

    fn to_deepseek(&self) -> Value {
        // DeepSeek - OpenAI-compatible format
        let functions: Vec<Value> = self
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            })
            .collect();
        json!({
            // DeepSeek's ids name a role rather than a release, so they follow the
            // current model rather than pinning one.
            "model": "deepseek-chat",
            "tools": functions,
            "tool_choice": "auto",
            "model_discovery": Self::model_discovery("https://api.deepseek.com/models"),
        })
    }

    /// The manifest as linked data, **including the tools it is a manifest of**.
    ///
    /// This arm ignored `self.tools` entirely. Every sibling exporter maps the
    /// catalogue into its format's tool schema; this one emitted six fixed keys
    /// naming the application and stopped, so `ironmon ai manifest --format
    /// jsonld` returned 295 bytes where `--format openai` returns 25 KB. Not an
    /// error and not an empty list with a reason: a well-formed document that
    /// describes the program and claims nothing about what it can do, offered
    /// in the CLI's help on equal footing with the formats that answer.
    ///
    /// `potentialAction` is schema.org's property for the actions an entity
    /// supports, which is what a tool is. The parameter schema has no
    /// schema.org vocabulary, so it goes under the `ironmon:` prefix the context
    /// already declares -- the same choice `ontology::jsonld` makes for
    /// `ironmon:unavailableReason`.
    fn to_json_ld(&self) -> Value {
        let actions: Vec<Value> = self
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "@type": "Action",
                    "name": tool.name,
                    "description": tool.description,
                    "ironmon:parameters": tool.parameters,
                })
            })
            .collect();
        json!({
            // The same namespace the ontology publishes, rather than a second
            // one. This read `https://schema.siliconmonitor.dev/` while
            // `ontology::jsonld` used `nervosys.github.io/.../ns#`, so the
            // `ironmon:` prefix meant two different vocabularies depending on
            // which command produced the document. The rename surfaced it; the
            // disagreement predates it.
            "@context": {
                "@vocab": "https://schema.org/",
                "ironmon": crate::ontology::jsonld::IRONMON_NS,
            },
            "@type": "SoftwareApplication",
            "name": self.name,
            "description": self.description,
            "applicationCategory": "SystemUtility",
            "softwareVersion": self.version,
            "potentialAction": actions
        })
    }

    fn to_mcp(&self) -> Value {
        let tools: Vec<Value> = self
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.parameters,
                })
            })
            .collect();
        json!({
            "name": "iron-monitor",
            "version": self.version,
            "description": self.description,
            "protocol_version": "2024-11-05",
            "capabilities": { "tools": { "listChanged": false } },
            "tools": tools
        })
    }
}

impl Default for AgentManifest {
    fn default() -> Self {
        Self::new()
    }
}

pub fn get_tools_by_category() -> HashMap<ToolCategory, Vec<ToolDefinition>> {
    let tools = get_all_tool_definitions();
    let mut grouped: HashMap<ToolCategory, Vec<ToolDefinition>> = HashMap::new();
    for tool in tools {
        grouped.entry(tool.category).or_default().push(tool);
    }
    grouped
}

#[cfg(test)]
mod model_staleness_tests {
    use super::{AgentManifest, ExportFormat};

    const ALL_FORMATS: &[ExportFormat] = &[
        ExportFormat::OpenAI,
        ExportFormat::Anthropic,
        ExportFormat::Gemini,
        ExportFormat::Grok,
        ExportFormat::Llama,
        ExportFormat::Mistral,
        ExportFormat::DeepSeek,
        ExportFormat::JsonLd,
        ExportFormat::Mcp,
        ExportFormat::SimpleJson,
    ];

    /// The manifest must not carry a frozen catalogue of model ids again.
    ///
    /// Seven of these formats used to ship a `supported_models` array. Each was a
    /// claim about the world on the day it was typed, and every one of them was
    /// wrong within months — the OpenAI entry still advertised `gpt-4o` and the
    /// Anthropic entry `claude-3-opus-20240229`. Nothing consumed them, and they
    /// were never true anyway: this manifest describes tools, and any tool-calling
    /// model can use it. If the key comes back, so does the rot.
    #[test]
    fn no_export_format_ships_a_frozen_model_catalogue() {
        let manifest = AgentManifest::new();
        for format in ALL_FORMATS {
            let exported = manifest.export(*format);
            assert!(
                exported.get("supported_models").is_none(),
                "{format:?} advertises a fixed list of model ids; name the \
                 provider's listing endpoint instead so the answer stays current"
            );
        }
    }

    /// A format that suggests a model must also say how to find a better one, so an
    /// agent reading the manifest is never stuck with the suggestion.
    #[test]
    fn a_suggested_model_comes_with_a_way_to_discover_others() {
        let manifest = AgentManifest::new();
        for format in ALL_FORMATS {
            let exported = manifest.export(*format);
            if exported.get("model").is_some() {
                assert!(
                    exported.get("model_discovery").is_some(),
                    "{format:?} names a default model but gives an agent no way to \
                     learn what else is available"
                );
            }
        }
    }
    /// Every format is a manifest *of the tools*, so every format must carry
    /// them.
    ///
    /// `to_json_ld` did not. It emitted six fixed keys about the application
    /// and dropped the catalogue, and nothing noticed because the document was
    /// valid, the CLI exited zero, and the only test over these formats checked
    /// that they had stopped shipping a frozen model list. A consumer asking
    /// for linked data got a well-formed answer with no tools in it.
    ///
    /// This counts occurrences of a tool's name in the serialised output rather
    /// than knowing each format's shape, deliberately: the assertion should
    /// hold for a format added next year by someone who never reads this file.
    #[test]
    fn every_format_carries_every_tool() {
        let manifest = AgentManifest::new();
        assert!(
            !manifest.tools.is_empty(),
            "the manifest has no tools, so the assertion below would pass \
             vacuously for every format"
        );
        let names: Vec<String> = manifest.tools.iter().map(|t| t.name.clone()).collect();

        let mut missing: Vec<String> = Vec::new();
        for format in ALL_FORMATS {
            let text = manifest.export(*format).to_string();
            let absent: Vec<&String> = names
                .iter()
                .filter(|n| !text.contains(n.as_str()))
                .collect();
            if !absent.is_empty() {
                missing.push(format!(
                    "{format:?} omits {} of {} tools, starting with {}",
                    absent.len(),
                    names.len(),
                    absent[0]
                ));
            }
        }

        assert!(
            missing.is_empty(),
            "a manifest format dropped the tools it is a manifest of:\n  {}",
            missing.join("\n  ")
        );
    }
}

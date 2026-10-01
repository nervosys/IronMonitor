//! Model Context Protocol (MCP) Server
//!
//! MCP server that allows AI agents like Claude to directly interact with hardware monitoring.

use super::AiDataApi;
use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};

pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

pub struct McpServer {
    api: AiDataApi,
    server_info: ServerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub const PARSE_ERROR: i32 = -32700;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

const RESOURCES: [(&str, &str, &str); 4] = [
    (
        "ironmon://system/summary",
        "System Summary",
        "get_system_summary",
    ),
    ("ironmon://gpu/status", "GPU Status", "get_gpu_status"),
    ("ironmon://cpu/status", "CPU Status", "get_cpu_status"),
    (
        "ironmon://memory/status",
        "Memory Status",
        "get_memory_status",
    ),
];

impl McpServer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            api: AiDataApi::new()?,
            server_info: ServerInfo {
                name: env!("CARGO_PKG_NAME").to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        })
    }

    pub fn run_stdio(&mut self) -> Result<()> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let reader = BufReader::new(stdin.lock());

        for line in reader.lines() {
            let line = line.map_err(|e| IronError::Io(std::io::Error::other(e)))?;
            if line.trim().is_empty() {
                continue;
            }

            let response = match serde_json::from_str::<McpRequest>(&line) {
                Ok(request) => self.handle_request(request),
                Err(e) => McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: None,
                    result: None,
                    error: Some(McpError {
                        code: PARSE_ERROR,
                        message: format!("Parse error: {}", e),
                        data: None,
                    }),
                },
            };

            let response_json = serde_json::to_string(&response)
                .map_err(|e| IronError::Io(std::io::Error::other(e)))?;
            writeln!(stdout, "{}", response_json)
                .map_err(|e| IronError::Io(std::io::Error::other(e)))?;
            stdout
                .flush()
                .map_err(|e| IronError::Io(std::io::Error::other(e)))?;
        }
        Ok(())
    }

    pub fn handle_request(&mut self, request: McpRequest) -> McpResponse {
        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(),
            "initialized" => Ok(json!({})),
            "tools/list" => self.handle_tools_list(),
            "tools/call" => self.handle_tools_call(&request.params),
            "resources/list" => self.handle_resources_list(),
            "resources/read" => self.handle_resources_read(&request.params),
            "ping" => Ok(json!({})),
            _ => Err(McpError {
                code: METHOD_NOT_FOUND,
                message: format!("Method not found: {}", request.method),
                data: None,
            }),
        };

        match result {
            Ok(value) => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(value),
                error: None,
            },
            Err(error) => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(error),
            },
        }
    }

    fn handle_initialize(&self) -> std::result::Result<Value, McpError> {
        Ok(json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "serverInfo": self.server_info,
            "capabilities": { "tools": { "listChanged": false }, "resources": { "subscribe": false, "listChanged": false } }
        }))
    }

    fn handle_tools_list(&self) -> std::result::Result<Value, McpError> {
        let tools: Vec<Value> = self.api.list_tools().iter().map(|t| {
            json!({ "name": t.name, "description": t.description, "inputSchema": t.parameters })
        }).collect();
        Ok(json!({ "tools": tools }))
    }

    fn handle_tools_call(&mut self, params: &Value) -> std::result::Result<Value, McpError> {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError {
                code: INVALID_PARAMS,
                message: "Missing tool name".to_string(),
                data: None,
            })?;
        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        match self.api.call_tool(name, arguments) {
            Ok(result) => Ok(json!({
                "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_else(|_| "".to_string()) }]
            })),
            Err(e) => Ok(
                json!({ "content": [{ "type": "text", "text": format!("Error: {}", e) }], "isError": true }),
            ),
        }
    }

    fn handle_resources_list(&self) -> std::result::Result<Value, McpError> {
        Ok(json!({
            "resources": RESOURCES.iter().map(|(uri,name,_)|
                json!({"uri":uri,"name":name,"mimeType":"application/json"})).collect::<Vec<_>>()
        }))
    }

    fn handle_resources_read(&mut self, params: &Value) -> std::result::Result<Value, McpError> {
        let uri = params
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| McpError {
                code: INVALID_PARAMS,
                message: "Missing resource URI".into(),
                data: None,
            })?;
        let (_, _, tool) = RESOURCES
            .iter()
            .find(|(known, _, _)| *known == uri)
            .ok_or_else(|| McpError {
                code: -32002,
                message: format!("Resource not found: {uri}"),
                data: None,
            })?;
        let result = self
            .api
            .call_tool(tool, json!({}))
            .map_err(|error| McpError {
                code: INTERNAL_ERROR,
                message: error.to_string(),
                data: None,
            })?;
        // Retain the tool result envelope: unavailable hardware is reported as
        // a failed reading with a reason, rather than becoming an empty resource.
        let text = serde_json::to_string(&result).map_err(|error| McpError {
            code: INTERNAL_ERROR,
            message: error.to_string(),
            data: None,
        })?;
        Ok(json!({"contents":[{"uri":uri,"mimeType":"application/json","text":text}]}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_advertised_resource_can_be_read_through_its_registered_tool() {
        let mut server = McpServer {
            api: AiDataApi::with_components(None, None, None),
            server_info: ServerInfo {
                name: "fixture".into(),
                version: "1".into(),
            },
        };
        let resources = server.handle_resources_list().unwrap();
        for row in resources["resources"].as_array().unwrap() {
            let result = server.handle_request(McpRequest {
                jsonrpc: "2.0".into(),
                id: Some(json!(1)),
                method: "resources/read".into(),
                params: json!({"uri":row["uri"]}),
            });
            assert!(result.error.is_none(), "{result:?}");
            let contents = &result.result.unwrap()["contents"][0];
            assert_eq!(contents["uri"], row["uri"]);
            assert_eq!(contents["mimeType"], "application/json");
            let reading: Value = serde_json::from_str(contents["text"].as_str().unwrap()).unwrap();
            assert!(reading["success"].is_boolean());
            if reading["success"] == true {
                assert!(!reading["data"].is_null());
            } else {
                assert!(reading["error"].as_str().is_some_and(|s| !s.is_empty()));
            }
        }
    }

    #[test]
    fn missing_and_unknown_resource_uris_have_distinct_errors() {
        let mut server = McpServer {
            api: AiDataApi::with_components(None, None, None),
            server_info: ServerInfo {
                name: "fixture".into(),
                version: "1".into(),
            },
        };
        assert_eq!(
            server.handle_resources_read(&json!({})).unwrap_err().code,
            INVALID_PARAMS
        );
        assert_eq!(
            server
                .handle_resources_read(&json!({"uri":"ironmon://missing"}))
                .unwrap_err()
                .code,
            -32002
        );
    }
}

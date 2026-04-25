use crate::ai::endpoints::AiEndpoint;
use crate::message::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Model Context Protocol (MCP) tool definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// MCP capability manifest for an endpoint.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpManifest {
    pub tools: Vec<McpTool>,
    pub resources: Vec<String>,
    pub prompts: Vec<String>,
}

/// Client for interacting with MCP-compatible endpoints.
#[derive(Debug, Clone)]
pub struct McpClient {
    http: reqwest::Client,
    endpoint: AiEndpoint,
}

impl McpClient {
    pub fn new(endpoint: AiEndpoint) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint,
        }
    }

    /// Discover tools from an MCP endpoint.
    pub async fn discover_tools(&self) -> anyhow::Result<Vec<McpTool>> {
        let url = format!("{}/mcp/tools", self.endpoint.url.trim_end_matches('/'));
        let resp = self.http.get(&url).send().await?;
        let manifest: McpManifest = resp.json().await?;
        Ok(manifest.tools)
    }

    /// Execute a tool call through an MCP endpoint.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let url = format!(
            "{}/mcp/tools/{}/call",
            self.endpoint.url.trim_end_matches('/'),
            tool_name
        );
        let resp = self
            .http
            .post(&url)
            .json(&arguments)
            .send()
            .await?;
        let result: serde_json::Value = resp.json().await?;
        Ok(result)
    }

    /// Route a message to an MCP endpoint if it matches a tool pattern.
    pub async fn route_message(&self, msg: &Message) -> anyhow::Result<Option<serde_json::Value>> {
        if msg.msg_type != "mcp_tool_call" {
            return Ok(None);
        }

        let tool_name = msg
            .data
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let args = msg.data.get("arguments").cloned().unwrap_or_default();

        let result = self.call_tool(tool_name, args).await?;
        Ok(Some(result))
    }
}

/// Registry of MCP clients indexed by endpoint id.
#[derive(Debug, Clone, Default)]
pub struct McpRegistry {
    clients: HashMap<String, McpClient>,
    /// tool_name -> endpoint_id
    tool_index: HashMap<String, String>,
}

impl McpRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, endpoint: AiEndpoint) {
        let id = endpoint.id.clone();
        self.clients.insert(id, McpClient::new(endpoint));
    }

    pub fn get(&self, endpoint_id: &str) -> Option<&McpClient> {
        self.clients.get(endpoint_id)
    }

    pub async fn discover_all(&self) -> HashMap<String, Vec<McpTool>> {
        let mut results = HashMap::new();
        for (id, client) in &self.clients {
            match client.discover_tools().await {
                Ok(tools) => {
                    results.insert(id.clone(), tools);
                }
                Err(e) => {
                    tracing::warn!("mcp discover failed for {}: {}", id, e);
                }
            }
        }
        results
    }

    /// Discover tools from all endpoints and build a tool-name index.
    pub async fn discover_and_index(&mut self) {
        self.tool_index.clear();
        for (id, client) in &self.clients {
            match client.discover_tools().await {
                Ok(tools) => {
                    for tool in tools {
                        if self.tool_index.contains_key(&tool.name) {
                            tracing::warn!(
                                "tool '{}' exists on multiple endpoints; using first discovered ({})",
                                tool.name,
                                id
                            );
                        } else {
                            self.tool_index.insert(tool.name, id.clone());
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("mcp discover failed for {}: {}", id, e);
                }
            }
        }
        tracing::info!("mcp tool index built: {} tools", self.tool_index.len());
    }

    /// Find the endpoint id that provides a given tool.
    pub fn find_endpoint_for_tool(&self, tool_name: &str) -> Option<&String> {
        self.tool_index.get(tool_name)
    }

    /// Execute a tool call by looking up the right endpoint.
    pub async fn call_tool_by_name(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let endpoint_id = match self.find_endpoint_for_tool(tool_name) {
            Some(id) => id,
            None => return Ok(None),
        };
        let client = match self.clients.get(endpoint_id) {
            Some(c) => c,
            None => return Ok(None),
        };
        let result = client.call_tool(tool_name, arguments).await?;
        Ok(Some(result))
    }
}

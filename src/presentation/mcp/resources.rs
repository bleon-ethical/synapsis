//! MCP resource definitions
use serde_json::{json, Value};

/// List all available resources
pub fn list_resources() -> Value {
    json!([
        { "uri": "synapsis://memory", "name": "Memory" },
        { "uri": "synapsis://skills", "name": "Skills" },
        { "uri": "synapsis://agents", "name": "Agents" }
    ])
}

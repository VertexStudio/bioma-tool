use crate::schema::{CallToolResult, TextContent};
use crate::tools::{ToolDef, ToolError};
use lazy_static::lazy_static;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::debug;

// Global memory store
lazy_static! {
    static ref MEMORY_STORE: Mutex<HashMap<String, Value>> = Mutex::new(HashMap::new());
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action")]
pub enum MemoryProperties {
    #[serde(rename = "store")]
    Store {
        #[schemars(description = "The key to store the memory under")]
        key: String,
        #[schemars(description = "The JSON value to store")]
        value: Value,
    },
    #[serde(rename = "retrieve")]
    Retrieve {
        #[schemars(description = "The key to retrieve the memory for")]
        key: String,
    },
    #[serde(rename = "list")]
    List,
    #[serde(rename = "delete")]
    Delete {
        #[schemars(description = "The key to delete from memory")]
        key: String,
    },
    #[serde(rename = "clear")]
    Clear,
}

#[derive(Clone, Debug, Serialize)]
pub struct Memory;

impl ToolDef for Memory {
    const NAME: &'static str = "memory";
    const DESCRIPTION: &'static str = "Store and retrieve JSON memories using string keys";
    type Properties = MemoryProperties;

    async fn call(&self, properties: Self::Properties) -> Result<CallToolResult, ToolError> {
        let mut store = MEMORY_STORE
            .lock()
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let result = match properties {
            MemoryProperties::Store { key, value } => {
                store.insert(key.clone(), value);
                format!("Successfully stored memory with key: {}", key)
            }
            MemoryProperties::Retrieve { key } => match store.get(&key) {
                Some(value) => serde_json::to_string_pretty(value)
                    .map_err(|e| ToolError::ResultSerialize(e))?,
                None => format!("No memory found for key: {}", key),
            },
            MemoryProperties::List => {
                let keys: Vec<&String> = store.keys().collect();
                serde_json::to_string_pretty(&keys).map_err(|e| ToolError::ResultSerialize(e))?
            }
            MemoryProperties::Delete { key } => match store.remove(&key) {
                Some(_) => format!("Successfully deleted memory with key: {}", key),
                None => format!("No memory found to delete for key: {}", key),
            },
            MemoryProperties::Clear => {
                store.clear();
                "Successfully cleared all memories".to_string()
            }
        };

        Ok(CallToolResult {
            content: vec![serde_json::to_value(TextContent {
                type_: "text".to_string(),
                text: result,
                annotations: None,
            })
            .map_err(ToolError::ResultSerialize)?],
            is_error: Some(false),
            meta: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolCallHandler;
    use serde_json::json;

    #[tokio::test]
    async fn test_memory_operations() {
        let tool = Memory;

        // Test storing
        let store_props = MemoryProperties::Store {
            key: "test_key".to_string(),
            value: json!({"test": "value"}),
        };
        let result = tool.call(store_props).await.unwrap();
        assert!(result.content[0]["text"]
            .as_str()
            .unwrap()
            .contains("Successfully stored"));

        // Test retrieving
        let retrieve_props = MemoryProperties::Retrieve {
            key: "test_key".to_string(),
        };
        let result = tool.call(retrieve_props).await.unwrap();
        assert!(result.content[0]["text"].as_str().unwrap().contains("test"));

        // Test listing
        let list_props = MemoryProperties::List;
        let result = tool.call(list_props).await.unwrap();
        assert!(result.content[0]["text"]
            .as_str()
            .unwrap()
            .contains("test_key"));

        // Test deleting
        let delete_props = MemoryProperties::Delete {
            key: "test_key".to_string(),
        };
        let result = tool.call(delete_props).await.unwrap();
        assert!(result.content[0]["text"]
            .as_str()
            .unwrap()
            .contains("Successfully deleted"));

        // Test clearing
        let store_props = MemoryProperties::Store {
            key: "test_key2".to_string(),
            value: json!({"test": "value"}),
        };
        tool.call(store_props).await.unwrap();

        let clear_props = MemoryProperties::Clear;
        let result = tool.call(clear_props).await.unwrap();
        assert!(result.content[0]["text"]
            .as_str()
            .unwrap()
            .contains("Successfully cleared"));

        // Verify memory is empty after clear
        let list_props = MemoryProperties::List;
        let result = tool.call(list_props).await.unwrap();
        assert_eq!(result.content[0]["text"].as_str().unwrap(), "[]");
    }

    #[test]
    fn test_memory_schema() {
        let tool = Memory.def();
        assert_eq!(tool.name, "memory");
        assert_eq!(
            tool.description.unwrap(),
            "Store and retrieve JSON memories using string keys"
        );
    }
}

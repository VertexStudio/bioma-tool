use crate::schema::{CallToolResult, TextContent, Tool, ToolInputSchema};
use crate::tools::{ToolDef, ToolError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const FETCH_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "url": {
            "description": "URL to fetch",
            "type": "string"
        },
        "max_length": {
            "description": "Maximum number of characters to return",
            "type": "integer",
            "default": 5000
        },
        "start_index": {
            "description": "Start content from this character index",
            "type": "integer",
            "default": 0
        },
        "raw": {
            "description": "Get raw content without markdown conversion",
            "type": "boolean",
            "default": false
        }
    },
    "required": ["url"]
}"#;

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct FetchProperties {
    #[schemars(description = "URL to fetch", required = true)]
    url: String,
    #[schemars(description = "Maximum number of characters to return")]
    max_length: Option<usize>,
    #[schemars(description = "Start content from this character index")]
    start_index: Option<usize>,
    #[schemars(description = "Get raw content without markdown conversion")]
    raw: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Fetch;

impl ToolDef for Fetch {
    const NAME: &'static str = "fetch";
    const DESCRIPTION: &'static str =
        "Fetches a URL from the internet and extracts its contents as markdown";
    type Properties = FetchProperties;

    fn def() -> Tool {
        let input_schema = serde_json::from_str::<ToolInputSchema>(FETCH_SCHEMA).unwrap();
        Tool {
            name: Self::NAME.to_string(),
            description: Some(Self::DESCRIPTION.to_string()),
            input_schema,
        }
    }

    async fn call(&self, properties: Self::Properties) -> Result<CallToolResult, ToolError> {
        let client = reqwest::Client::new();

        // Fetch the webpage
        let response = match client.get(&properties.url).send().await {
            Ok(resp) => resp,
            Err(e) => return Ok(Self::error(format!("Failed to fetch URL: {}", e))),
        };

        // Get the HTML content
        let html = match response.text().await {
            Ok(text) => text,
            Err(e) => return Ok(Self::error(format!("Failed to get response text: {}", e))),
        };

        // Process the content based on parameters
        let content = if properties.raw.unwrap_or(false) {
            html
        } else {
            html2md::parse_html(&html)
        };

        // Apply start_index and max_length
        let start = properties.start_index.unwrap_or(0);
        let content = if start < content.len() {
            &content[start..]
        } else {
            ""
        };

        let content = if let Some(max_length) = properties.max_length {
            if max_length < content.len() {
                &content[..max_length]
            } else {
                content
            }
        } else {
            &content[..content.len().min(5000)]
        };

        Ok(Self::success(content))
    }
}

impl Fetch {
    fn error(error_message: impl Into<String>) -> CallToolResult {
        CallToolResult {
            content: vec![serde_json::to_value(TextContent {
                type_: "text".to_string(),
                text: error_message.into(),
                annotations: None,
            })
            .unwrap_or_default()],
            is_error: Some(true),
            meta: None,
        }
    }

    fn success(message: impl Into<String>) -> CallToolResult {
        CallToolResult {
            content: vec![serde_json::to_value(TextContent {
                type_: "text".to_string(),
                text: message.into(),
                annotations: None,
            })
            .unwrap_or_default()],
            is_error: Some(false),
            meta: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolCallHandler;

    #[tokio::test]
    async fn test_web_browser_tool() {
        let tool = Fetch;
        let props = FetchProperties {
            url: "https://example.com".to_string(),
            max_length: None,
            start_index: None,
            raw: None,
        };

        let result = tool.call(props).await.unwrap();
        let content = result.content[0]["text"].as_str().unwrap();

        // Check that the markdown contains some expected content from example.com
        assert!(content.contains("Example Domain"));
        assert_eq!(result.is_error, Some(false));
    }

    #[test]
    fn test_web_browser_schema() {
        let tool = Fetch.def();
        assert_eq!(tool.name, "fetch");
        assert_eq!(
            tool.description.unwrap(),
            "Fetches a URL from the internet and extracts its contents as markdown"
        );

        let schema = tool.input_schema;
        assert_eq!(schema.type_, "object");

        let props = schema.properties.unwrap();
        assert!(props.contains_key("url"));

        let required = schema.required.unwrap();
        assert!(required.contains(&"url".to_string()));
    }
}

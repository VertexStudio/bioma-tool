use crate::schema::{CallToolResult, TextContent, Tool, ToolInputSchema};
use crate::tools::{ToolDef, ToolError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const WEB_BROWSER_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "url": {
            "description": "The URL of the webpage to read",
            "type": "string"
        }
    },
    "required": ["url"]
}"#;

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct WebBrowserProperties {
    #[schemars(description = "The URL of the webpage to read", required = true)]
    url: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct WebBrowser;

impl ToolDef for WebBrowser {
    const NAME: &'static str = "web_browser";
    const DESCRIPTION: &'static str = "Reads and returns the contents of a webpage";
    type Properties = WebBrowserProperties;

    fn def() -> Tool {
        let input_schema = serde_json::from_str::<ToolInputSchema>(WEB_BROWSER_SCHEMA).unwrap();
        Tool {
            name: Self::NAME.to_string(),
            description: Some(Self::DESCRIPTION.to_string()),
            input_schema,
        }
    }

    async fn call(&self, properties: Self::Properties) -> Result<CallToolResult, ToolError> {
        // Create HTTP client
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

        // Convert HTML to markdown
        let markdown = html2md::parse_html(&html);
        Ok(Self::success(markdown))
    }
}

impl WebBrowser {
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
        let tool = WebBrowser;
        let props = WebBrowserProperties {
            url: "https://example.com".to_string(),
        };

        let result = tool.call(props).await.unwrap();
        let content = result.content[0]["text"].as_str().unwrap();

        // Check that the markdown contains some expected content from example.com
        assert!(content.contains("Example Domain"));
        assert_eq!(result.is_error, Some(false));
    }

    #[test]
    fn test_web_browser_schema() {
        let tool = WebBrowser.def();
        assert_eq!(tool.name, "web_browser");
        assert_eq!(
            tool.description.unwrap(),
            "Reads and returns the contents of a webpage"
        );

        let schema = tool.input_schema;
        assert_eq!(schema.type_, "object");

        let props = schema.properties.unwrap();
        assert!(props.contains_key("url"));

        let required = schema.required.unwrap();
        assert!(required.contains(&"url".to_string()));
    }
}

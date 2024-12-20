use crate::schema::{CallToolResult, TextContent};
use crate::tools::{ToolDef, ToolError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct EchoProperties {
    #[schemars(description = "The message to echo", required = true)]
    message: String,
}

#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct Echo;

impl ToolDef for Echo {
    const NAME: &'static str = "echo";
    const DESCRIPTION: &'static str = "Echoes back the input message";
    type Properties = EchoProperties;

    async fn call(&self, properties: Self::Properties) -> Result<CallToolResult, ToolError> {
        Ok(CallToolResult {
            content: vec![serde_json::to_value(TextContent {
                type_: "text".to_string(),
                text: properties.message,
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
    use crate::tools::{ToolCallHandler, ToolDef};

    #[tokio::test]
    async fn test_echo_tool() {
        let tool = Echo;
        let props = EchoProperties {
            message: "hello".to_string(),
        };

        let result = ToolDef::call(&tool, props).await.unwrap();
        assert_eq!(result.content[0]["text"].as_str().unwrap(), "hello");
        assert_eq!(result.is_error, Some(false));
    }

    #[test]
    fn test_echo_schema() {
        let tool = Echo.def();
        assert_eq!(tool.name, "echo");
        assert_eq!(tool.description.unwrap(), "Echoes back the input message");

        let schema = tool.input_schema;
        assert_eq!(schema.type_, "object");

        let props = schema.properties.unwrap();
        assert!(props.contains_key("message"));

        let required = schema.required.unwrap();
        assert!(required.contains(&"message".to_string()));
    }
}

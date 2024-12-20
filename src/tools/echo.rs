use crate::tools::{ToolDef, ToolError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

    fn call(&self, properties: Self::Properties) -> Result<Value, ToolError> {
        serde_json::to_value(properties.message).map_err(ToolError::ResultSerialize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{ToolCallHandler, ToolDef};

    #[test]
    fn test_echo_tool() {
        let tool = Echo;
        let props = EchoProperties {
            message: "hello".to_string(),
        };
        let result = ToolDef::call(&tool, props).unwrap();
        assert_eq!(result.as_str().unwrap(), "hello");
    }

    #[test]
    fn test_echo_schema() {
        let tool = Echo.def();
        assert_eq!(tool.name, "echo");
        assert_eq!(tool.description.unwrap(), "Echoes back the input");

        let schema = tool.input_schema;
        assert_eq!(schema.type_, "object");

        let props = schema.properties.unwrap();
        assert!(props.contains_key("message"));

        let required = schema.required.unwrap();
        assert!(required.contains(&"message".to_string()));
    }
}

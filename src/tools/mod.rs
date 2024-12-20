use crate::schema::{self, ToolInputSchema};
use schemars::{schema_for, JsonSchema};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub mod echo;

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Failed to parse tool arguments: {0}")]
    ArgumentParse(serde_json::Error),

    #[error("Tool execution failed: {0}")]
    Execution(String),

    #[error("Failed to serialize tool result: {0}")]
    ResultSerialize(serde_json::Error),
}

pub trait ToolCallHandler: Send + Sync {
    fn call(&self, args: Value) -> Result<Value, ToolError>;
    fn def(&self) -> schema::Tool;
}

pub trait ToolDef: Serialize {
    const NAME: &'static str;
    const DESCRIPTION: &'static str;
    type Properties: Serialize + JsonSchema + serde::de::DeserializeOwned;

    fn def() -> schema::Tool {
        let schema = schema_for!(Self::Properties);
        let schema_value = serde_json::to_value(schema).unwrap();

        let tool_input_schema = ToolInputSchema {
            type_: schema_value["type"]
                .as_str()
                .unwrap_or("object")
                .to_string(),
            properties: schema_value["properties"].as_object().map(|props| {
                props
                    .iter()
                    .map(|(prop_name, prop_value)| {
                        let inner_map = prop_value
                            .as_object()
                            .unwrap() // Safe because JsonSchema generates valid objects
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect::<BTreeMap<String, Value>>();

                        (prop_name.clone(), inner_map)
                    })
                    .collect::<BTreeMap<String, BTreeMap<String, Value>>>()
            }),
            required: schema_value["required"].as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<String>>()
            }),
        };

        schema::Tool {
            name: Self::NAME.to_string(),
            description: Some(Self::DESCRIPTION.to_string()),
            input_schema: tool_input_schema,
        }
    }

    fn call(&self, properties: Self::Properties) -> Result<Value, ToolError>;
}

impl<T: ToolDef + Send + Sync> ToolCallHandler for T {
    fn call(&self, args: Value) -> Result<Value, ToolError> {
        let properties = serde_json::from_value(args).map_err(ToolError::ArgumentParse)?;
        self.call(properties)
    }

    fn def(&self) -> schema::Tool {
        T::def()
    }
}

use crate::schema::{self, CallToolResult, ToolInputSchema};
use schemars::{schema_for, JsonSchema};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

/// Modules containing tool implementations
pub mod echo;
pub mod memory;

/// Errors that can occur during tool operations
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// Error when parsing tool arguments from JSON
    #[error("Failed to parse tool arguments: {0}")]
    ArgumentParse(serde_json::Error),

    /// Error during tool execution
    #[error("Tool execution failed: {0}")]
    Execution(String),

    /// Error when serializing tool results to JSON
    #[error("Failed to serialize tool result: {0}")]
    ResultSerialize(serde_json::Error),
}

/// Trait for handling tool calls with dynamic dispatch
///
/// This trait provides an interface for executing tools with serialized arguments
/// and returning serialized results.
pub trait ToolCallHandler: Send + Sync {
    /// Executes the tool with the given arguments
    ///
    /// # Arguments
    /// * `args` - Optional map of argument names to JSON values
    ///
    /// # Returns
    /// A future that resolves to either a tool result or an error
    fn call_boxed<'a>(
        &'a self,
        args: Option<BTreeMap<String, Value>>,
    ) -> Pin<Box<dyn Future<Output = Result<CallToolResult, ToolError>> + Send + 'a>>;

    /// Returns the tool's definition/schema
    fn def(&self) -> schema::Tool;
}

/// Trait for defining a concrete tool implementation
///
/// This trait provides a way to define a tool's metadata and implementation
/// with strongly-typed arguments.
///
/// # JSON Schema Properties
/// 
/// The `schemars` crate supports standard JSON Schema attributes through the `schemars` attribute macro.
/// See the [JSON Schema Reference](https://json-schema.org/understanding-json-schema) for full documentation.
///
/// Common attributes include:
/// ```rust
/// #[derive(JsonSchema, Serialize, Deserialize)]
/// struct ExampleProperties {
///     #[schemars(description = "Description of the field")]
///     #[schemars(required = true)]
///     basic_field: String,
///
///     #[schemars(minimum = 0, maximum = 100)]
///     #[schemars(example = 42)]
///     number_field: i32,
///
///     #[schemars(regex = "^[a-zA-Z]+$")]
///     #[schemars(length(min = 1, max = 50))]
///     string_field: String,
///
///     #[schemars(default = true)]
///     optional_field: bool,
///
///     #[schemars(enum_values = ["one", "two", "three"])]
///     enum_field: String,
/// }
/// ```
///
/// Additional resources:
/// - [schemars documentation](https://docs.rs/schemars)
/// - [JSON Schema Validation](https://json-schema.org/draft/2020-12/json-schema-validation.html)
pub trait ToolDef: Serialize {
    /// The name of the tool
    const NAME: &'static str;
    
    /// A description of what the tool does
    const DESCRIPTION: &'static str;
    
    /// The type representing the tool's input properties
    type Properties: Serialize + JsonSchema + serde::de::DeserializeOwned;

    /// Generates the tool's schema definition
    ///
    /// This method creates a complete tool schema including name, description,
    /// and input parameter definitions derived from the Properties type.
    fn def() -> schema::Tool {
        let schema = schema_for!(Self::Properties);
        let schema_value = serde_json::to_value(schema).unwrap();

        let tool_input_schema = if let Some(_discriminator) = schema_value.get("discriminator") {
            // Handle tagged enum case
            let variants = schema_value["oneOf"]
                .as_array()
                .unwrap()
                .iter()
                .map(|variant| {
                    let properties: BTreeMap<String, BTreeMap<String, Value>> = variant["properties"]
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(prop_name, prop_value)| {
                            let inner_map: BTreeMap<String, Value> = prop_value
                                .as_object()
                                .unwrap()
                                .iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();
                            (prop_name.clone(), inner_map)
                        })
                        .collect();

                    let required: Option<Vec<String>> = variant["required"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        });

                    (properties, required)
                })
                .fold(
                    (BTreeMap::new(), Vec::new()),
                    |(mut props, mut reqs), (variant_props, variant_reqs)| {
                        props.extend(variant_props);
                        if let Some(variant_reqs) = variant_reqs {
                            reqs.extend(variant_reqs);
                        }
                        (props, reqs)
                    },
                );

            ToolInputSchema {
                type_: "object".to_string(),
                properties: Some(variants.0),
                required: Some(variants.1),
            }
        } else {
            // Original handling for regular objects
            ToolInputSchema {
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
                                .unwrap()
                                .iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();
                            (prop_name.clone(), inner_map)
                        })
                        .collect()
                }),
                required: schema_value["required"].as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                }),
            }
        };

        schema::Tool {
            name: Self::NAME.to_string(),
            description: Some(Self::DESCRIPTION.to_string()),
            input_schema: tool_input_schema,
        }
    }

    /// Executes the tool with strongly-typed properties
    ///
    /// # Arguments
    /// * `properties` - The typed input properties for the tool
    ///
    /// # Returns
    /// A future that resolves to either a tool result or an error
    fn call<'a>(
        &'a self,
        properties: Self::Properties,
    ) -> impl Future<Output = Result<CallToolResult, ToolError>> + Send + 'a;
}

/// Implementation of `ToolCallHandler` for any type implementing `ToolDef`
///
/// This provides the bridge between the dynamic dispatch interface and
/// the concrete tool implementation.
impl<T: ToolDef + Send + Sync> ToolCallHandler for T {
    fn call_boxed<'a>(
        &'a self,
        args: Option<BTreeMap<String, Value>>,
    ) -> Pin<Box<dyn Future<Output = Result<CallToolResult, ToolError>> + Send + 'a>> {
        Box::pin(async move {
            let value = match args {
                Some(map) => serde_json::to_value(map).map_err(ToolError::ArgumentParse)?,
                None => Value::Null,
            };

            let properties: T::Properties =
                serde_json::from_value(value).map_err(ToolError::ArgumentParse)?;

            self.call(properties).await
        })
    }

    fn def(&self) -> schema::Tool {
        T::def()
    }
}

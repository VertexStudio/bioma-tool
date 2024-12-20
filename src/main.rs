use anyhow::{Context, Result};
use clap::Parser;
use jsonrpc_core::{MetaIoHandler, Metadata, Params};
use log::{debug, error, info, LevelFilter};
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
};
use serde::Serialize;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tools::ToolCallHandler;
use transport::{StdioTransport, Transport, TransportType, WebSocketTransport};

pub mod schema;
pub mod tools;
mod transport;

use schema::{
    CallToolRequestParams, CancelledNotificationParams, Implementation, InitializeRequestParams,
    InitializeResult, ListPromptsResult, ListResourcesResult, ListToolsResult, Prompt,
    PromptArgument, Resource, ServerCapabilities, ServerCapabilitiesPrompts,
    ServerCapabilitiesPromptsResources, ServerCapabilitiesPromptsResourcesTools,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the log file
    #[arg(long, default_value = "mcp_server.log")]
    log_file: PathBuf,

    /// Transport type (stdio or websocket)
    #[arg(long, default_value = "stdio")]
    transport: String,

    /// WebSocket address (only used with websocket transport)
    #[arg(long, default_value = "127.0.0.1:8080")]
    ws_addr: String,
}

struct McpServer {
    tools: Vec<Box<dyn ToolCallHandler>>,
    resources: Vec<Resource>,
    prompts: Vec<Prompt>,
}

impl McpServer {
    fn new() -> Self {
        let example_resource = Resource {
            name: "example.txt".to_string(),
            uri: "file:///example.txt".to_string(),
            description: Some("An example text file".to_string()),
            mime_type: Some("text/plain".to_string()),
            annotations: None,
        };

        let example_prompt = Prompt {
            name: "greet".to_string(),
            description: Some("A friendly greeting prompt".to_string()),
            arguments: Some(vec![PromptArgument {
                name: "name".to_string(),
                description: Some("Name of the person to greet".to_string()),
                required: Some(true),
            }]),
        };

        Self {
            tools: vec![Box::new(tools::echo::Echo)],
            resources: vec![example_resource],
            prompts: vec![example_prompt],
        }
    }

    fn get_capabilities(&self) -> ServerCapabilities {
        ServerCapabilities {
            tools: Some(ServerCapabilitiesPromptsResourcesTools {
                list_changed: Some(false),
            }),
            resources: Some(ServerCapabilitiesPromptsResources {
                list_changed: Some(false),
                subscribe: Some(false),
            }),
            prompts: Some(ServerCapabilitiesPrompts {
                list_changed: Some(false),
            }),
            ..Default::default()
        }
    }

    fn create_response<T: Serialize>(result: T) -> serde_json::Value {
        serde_json::to_value(result).unwrap_or_default()
    }
}

fn setup_logging(log_path: PathBuf) -> Result<()> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create log directory")?;
    }

    // Create file appender
    let file_appender = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d} - {l} - {m}{n}")))
        .build(log_path)
        .context("Failed to create log file appender")?;

    // Build logging configuration
    let config = Config::builder()
        .appender(Appender::builder().build("file", Box::new(file_appender)))
        .build(Root::builder().appender("file").build(LevelFilter::Debug))
        .context("Failed to build logging config")?;

    // Initialize logging
    log4rs::init_config(config).context("Failed to initialize logging")?;

    info!("Logging system initialized");
    Ok(())
}

#[derive(Default, Clone)]
struct ServerMetadata;
impl Metadata for ServerMetadata {}

pub async fn start_server(mut transport: TransportType) -> Result<()> {
    let server = McpServer::new();
    let mut io_handler = MetaIoHandler::default();

    let server = std::sync::Arc::new(server);
    let server_tools = server.clone();
    let server_resources = server.clone();
    let server_prompts = server.clone();
    let server_call = server.clone();

    io_handler.add_method_with_meta(
        "initialize",
        move |params: Params, _meta: ServerMetadata| {
            let server = server.clone();
            debug!("Handling initialize request");

            async move {
                let init_params: InitializeRequestParams = params.parse().map_err(|e| {
                    error!("Failed to parse initialize parameters: {}", e);
                    jsonrpc_core::Error::invalid_params(e.to_string())
                })?;

                let result = InitializeResult {
                    capabilities: server.get_capabilities(),
                    protocol_version: init_params.protocol_version,
                    server_info: Implementation {
                        name: "rust-mcp-server".to_string(),
                        version: "0.1.0".to_string(),
                    },
                    instructions: Some("Basic MCP server with tool support".to_string()),
                    meta: None,
                };

                info!("Successfully handled initialize request");
                Ok(serde_json::to_value(result).map_err(|e| {
                    error!("Failed to serialize initialize result: {}", e);
                    jsonrpc_core::Error::invalid_params(e.to_string())
                })?)
            }
        },
    );

    io_handler.add_notification_with_meta(
        "notifications/initialized",
        |_params, _meta: ServerMetadata| {
            info!("Received initialized notification");
        },
    );

    io_handler.add_notification_with_meta(
        "cancelled",
        move |params: Params, _meta: ServerMetadata| match params
            .parse::<CancelledNotificationParams>()
        {
            Ok(cancel_params) => {
                info!(
                    "Received cancellation for request {}: {}",
                    cancel_params.request_id,
                    cancel_params.reason.unwrap_or_default()
                );
            }
            Err(e) => {
                error!("Failed to parse cancellation params: {}", e);
            }
        },
    );

    io_handler.add_method("resources/list", move |_params| {
        let server = server_resources.clone();
        debug!("Handling resources/list request");

        async move {
            let response = ListResourcesResult {
                next_cursor: None,
                resources: server.resources.clone(),
                meta: None,
            };

            info!("Successfully handled resources/list request");
            Ok(McpServer::create_response(response))
        }
    });

    io_handler.add_method("prompts/list", move |_params| {
        let server = server_prompts.clone();
        debug!("Handling prompts/list request");

        async move {
            let response = ListPromptsResult {
                next_cursor: None,
                prompts: server.prompts.clone(),
                meta: None,
            };

            info!("Successfully handled prompts/list request");
            Ok(McpServer::create_response(response))
        }
    });

    io_handler.add_method("tools/list", move |_params| {
        let server = server_tools.clone();
        debug!("Handling tools/list request");

        let tools = server
            .tools
            .iter()
            .map(|tool| tool.def())
            .collect::<Vec<_>>();

        async move {
            let response = ListToolsResult {
                next_cursor: None,
                tools: tools,
                meta: None,
            };

            info!("Successfully handled tools/list request");
            Ok(McpServer::create_response(response))
        }
    });

    io_handler.add_method("tools/call", move |params: Params| {
        let server = server_call.clone();
        debug!("Handling tools/call request");

        async move {
            let params: CallToolRequestParams = params.parse().map_err(|e| {
                error!("Failed to parse tool call parameters: {}", e);
                jsonrpc_core::Error::invalid_params(e.to_string())
            })?;

            // Find the requested tool
            let tool = server.tools.iter().find(|t| t.def().name == params.name);

            match tool {
                Some(tool) => {
                    let result = tool.call_boxed(params.arguments).await.map_err(|e| {
                        error!("Tool execution failed: {}", e);
                        jsonrpc_core::Error::internal_error()
                    })?;

                    info!("Successfully handled tool call for: {}", params.name);
                    Ok(serde_json::to_value(result).map_err(|e| {
                        error!("Failed to serialize tool call result: {}", e);
                        jsonrpc_core::Error::invalid_params(e.to_string())
                    })?)
                }
                None => {
                    error!("Unknown tool requested: {}", params.name);
                    Err(jsonrpc_core::Error::method_not_found())
                }
            }
        }
    });

    let (tx, mut rx) = mpsc::channel(32);

    // Spawn the transport reader
    let mut transport_reader = transport.clone();
    tokio::spawn(async move {
        if let Err(e) = transport_reader.start(tx).await {
            error!("Transport error: {}", e);
        }
    });

    // Handle incoming messages
    while let Some(request) = rx.recv().await {
        let response = io_handler
            .handle_request(&request, ServerMetadata::default())
            .await
            .unwrap_or_else(|| {
                if !request.contains(r#""method":"notifications/"#) && 
                   !request.contains(r#""method":"cancelled"#) {
                    error!("Error handling request");
                    return r#"{"jsonrpc": "2.0", "error": {"code": -32603, "message": "Internal error"}, "id": null}"#.to_string();
                }
                String::new()
            });

        if !response.is_empty() {
            if let Err(e) = transport.send_response(response).await {
                error!("Failed to send response: {}", e);
                return Err(e).context("Failed to send response");
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    setup_logging(args.log_file)?;

    let transport = match args.transport.as_str() {
        "stdio" => TransportType::Stdio(StdioTransport::new()),
        "websocket" => TransportType::WebSocket(WebSocketTransport::new(args.ws_addr)),
        _ => return Err(anyhow::anyhow!("Invalid transport type")),
    };

    start_server(transport).await
}

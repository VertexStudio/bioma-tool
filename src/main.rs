use anyhow::{Context, Result};
use clap::Parser;
use jsonrpc_core::{IoHandler, Params};
use log::{debug, error, info, LevelFilter};
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
};
use serde::Serialize;
use std::path::PathBuf;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader},
    sync::mpsc,
};
use tools::ToolCallHandler;

pub mod schema;
pub mod tools;

use schema::{
    CallToolRequestParams, CallToolResult, CancelledNotificationParams, Implementation,
    InitializeRequestParams, InitializeResult, ListPromptsResult, ListResourcesResult,
    ListToolsResult, Prompt, PromptArgument, Resource, ServerCapabilities,
    ServerCapabilitiesPrompts, ServerCapabilitiesPromptsResources,
    ServerCapabilitiesPromptsResourcesTools, TextContent,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the log file
    #[arg(long, default_value = "mcp_server.log")]
    log_file: PathBuf,
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

pub async fn start_server() -> Result<()> {
    let server = McpServer::new();
    let mut io_handler = IoHandler::new();

    let server = std::sync::Arc::new(server);
    let server_tools = server.clone();
    let server_resources = server.clone();
    let server_prompts = server.clone();

    io_handler.add_method("initialize", move |params: Params| {
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
    });

    io_handler.add_notification("notifications/initialized", |_params| {
        info!("Received initialized notification");
    });

    io_handler.add_notification("cancelled", move |params: Params| match params
        .parse::<CancelledNotificationParams>(
    ) {
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
    });

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
        debug!("Handling tools/call request");

        async move {
            let params: CallToolRequestParams = params.parse().map_err(|e| {
                error!("Failed to parse tool call parameters: {}", e);
                jsonrpc_core::Error::invalid_params(e.to_string())
            })?;

            match params.name.as_str() {
                "echo" => {
                    let message = params
                        .arguments
                        .and_then(|v| v.get("message").cloned())
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        .ok_or_else(|| {
                            error!("Missing message argument in echo tool request");
                            jsonrpc_core::Error::invalid_params("Missing message argument")
                        })?;

                    let result = CallToolResult {
                        content: vec![serde_json::to_value(TextContent {
                            type_: "text".to_string(),
                            text: message,
                            annotations: None,
                        })
                        .unwrap()],
                        is_error: Some(false),
                        meta: None,
                    };

                    info!("Successfully handled echo tool call");
                    Ok(serde_json::to_value(result).map_err(|e| {
                        error!("Failed to serialize tool call result: {}", e);
                        jsonrpc_core::Error::invalid_params(e.to_string())
                    })?)
                }
                _ => {
                    error!("Unknown tool requested: {}", params.name);
                    Err(jsonrpc_core::Error::method_not_found())
                }
            }
        }
    });

    let (tx, mut rx) = mpsc::channel(32);

    tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let mut lines = TokioBufReader::new(stdin).lines();

        while let Ok(Some(line)) = lines.next_line().await {
            debug!("Received: {}", line);
            if tx.send(line).await.is_err() {
                error!("Failed to send request through channel");
                break;
            }
        }
    });

    let mut stdout = tokio::io::stdout();
    while let Some(line) = rx.recv().await {
        let response = io_handler
            .handle_request(&line)
            .await
            .unwrap_or_else(|| {
                if !line.contains(r#""method":"notifications/"#) && 
                   !line.contains(r#""method":"cancelled"#) {
                    error!("Error handling request");
                    return r#"{"jsonrpc": "2.0", "error": {"code": -32603, "message": "Internal error"}, "id": null}"#.to_string();
                }
                String::new()
            });

        if !response.is_empty() {
            debug!("Sending: {}", response);
            if let Err(e) = stdout.write_all(response.as_bytes()).await {
                error!("Failed to write response: {}", e);
                return Err(e).context("Failed to write response");
            }
            if let Err(e) = stdout.write_all(b"\n").await {
                error!("Failed to write newline: {}", e);
                return Err(e).context("Failed to write newline");
            }
            if let Err(e) = stdout.flush().await {
                error!("Failed to flush stdout: {}", e);
                return Err(e).context("Failed to flush stdout");
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    setup_logging(args.log_file)?;
    start_server().await
}


Configure the claude_config.json file
```
{
    "mcpServers": {
        "bioma-tool": {
            "command": "/Users/rozgo/BiomaAI/bioma-tool/target/debug/bioma-tool",
            "args": ["--log-file", "/Users/rozgo/BiomaAI/bioma-tool/mcp_server.log"]
        }
    }
}
```

Generate the schema.es from MCP schema.json
```
schemafy-cli src | rustfmt | tee src/schema.rs
```


use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use flute2_mcp::{
    config::{Config, ConfigError},
    runner::ProcessRunner,
    server::FluteServer,
};
use rmcp::{
    ServiceExt, model::ClientJsonRpcMessage, service::ServerInitializeError, transport::io::stdio,
};
use serde_json::{Value, json};
use tracing::Level;
use tracing_subscriber::{
    EnvFilter, filter::filter_fn, layer::SubscriberExt, util::SubscriberInitExt,
};

#[derive(Debug, Parser)]
#[command(
    name = "flute2-mcp",
    about = "MCP server for the flute2 payments CLI",
    version
)]
struct Args {
    /// Override `FLUTE2_PROFILE` (sandbox | production).
    #[arg(long, env = "FLUTE2_PROFILE")]
    profile: Option<String>,
    /// Override `FLUTE2_BIN` (path to the `flute2` binary).
    #[arg(long, env = "FLUTE2_BIN")]
    binary: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Runtime::new()?;
    let result = runtime.block_on(run());
    // The blocking read on stdin cannot be interrupted, so the runtime gets a
    // deadline rather than the wait-forever of a plain drop. Shutdown drops the
    // tasks, and with them any `flute2` child a cancelled call left running.
    runtime.shutdown_timeout(Duration::from_secs(1));
    result
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Logs go to stderr only: stdout is the MCP transport. `rmcp::service`
    // logs every `tools/call` with its arguments, card and bank numbers
    // included, so nothing `rmcp` emits below `info` is recorded, whatever
    // `RUST_LOG` asks for. `RUST_LOG` still decides everything at `info` and
    // above, for `rmcp` as for the rest.
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(filter_fn(|meta| {
            !(meta.target().starts_with("rmcp") && *meta.level() > Level::INFO)
        }))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let args = Args::parse();
    let cfg = Config::from_env(|name| match name {
        "FLUTE2_PROFILE" => args.profile.clone().or_else(|| std::env::var(name).ok()),
        "FLUTE2_BIN" => args.binary.clone().or_else(|| std::env::var(name).ok()),
        other => std::env::var(other).ok(),
    });

    let cfg = match cfg {
        Ok(c) => c,
        Err(ConfigError::BinaryNotFound) => {
            eprintln!(
                "flute2-mcp: could not find `flute2` on PATH. Install it from \
                 https://github.com/getflute/flute-cli-v2 or set FLUTE2_BIN."
            );
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("flute2-mcp: configuration error: {e}");
            std::process::exit(2);
        }
    };

    tracing::info!(
        profile = %cfg.profile.as_cli_str(),
        binary = %cfg.binary.display(),
        allow_prod_writes = cfg.allow_prod_writes,
        "starting flute2-mcp"
    );

    let runner = Arc::new(ProcessRunner {
        binary: cfg.binary.clone(),
        timeout: cfg.timeout,
        debug: cfg.debug,
    });
    let server = FluteServer::new(Arc::new(cfg), runner);

    let service = loop {
        match server.clone().serve(stdio()).await {
            Ok(service) => break service,
            // A frame that is not a usable `initialize`. Answer the client and
            // serve again, so a corrected handshake is accepted.
            Err(ServerInitializeError::ExpectedInitializeRequest(Some(msg))) => {
                let frame = handshake_error(&msg);
                tracing::warn!("{}", frame["error"]["message"].as_str().unwrap_or_default());
                // A notification carries no id and is never answered.
                if !frame["id"].is_null() {
                    println!("{frame}");
                }
            }
            Err(e) => return Err(e.into()),
        }
    };

    let shutdown = service.cancellation_token();
    tokio::spawn(async move {
        signalled().await;
        tracing::info!("signal received, cancelling in-flight calls");
        shutdown.cancel();
    });

    service.waiting().await?;
    Ok(())
}

/// A JSON-RPC error answering a frame that is not a usable `initialize`. It
/// carries the frame's own id and its method, and no other part of it: the
/// params of a pre-handshake `tools/call` hold card and bank numbers.
fn handshake_error(msg: &ClientJsonRpcMessage) -> Value {
    let frame = serde_json::to_value(msg).unwrap_or(Value::Null);
    let id = frame.get("id").cloned().unwrap_or(Value::Null);
    let (code, message) = match frame.get("method").and_then(Value::as_str) {
        // -32602, invalid params: the request is `initialize` and its params
        // are not the shape the handshake takes.
        Some("initialize") => (
            -32602,
            "invalid `initialize` params: `protocolVersion` is a string, and `capabilities` and `clientInfo` are both required"
                .to_string(),
        ),
        // -32600, invalid request: an `initialize` has to come first.
        Some(method) => (
            -32600,
            format!("expected `initialize` as the first request, received `{method}`"),
        ),
        None => (
            -32600,
            "expected `initialize` as the first request".to_string(),
        ),
    };
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// Resolves when the process is asked to stop. Cancelling the service drops
/// the in-flight call, and `kill_on_drop` takes the `flute2` child with it.
async fn signalled() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = signal(SignalKind::terminate()).expect("install a SIGTERM handler");
        tokio::select! {
            _ = term.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

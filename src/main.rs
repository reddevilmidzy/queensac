use queensac::{Application, KoreanTime, get_configuration};
use std::net::SocketAddr;
use tokio::net;
use tracing::{Level, error, info};

/// Initializes logging, loads configuration, builds the application, and starts the HTTP server.
///
/// This is the entry point for the queensac service. It sets up tracing, loads configuration,
/// constructs the application, binds to the specified port on localhost, and serves HTTP requests.
/// On any critical failure during startup, the process logs the error and exits.
///
/// # Examples
///
/// ```no_run
/// // Run the application (typically executed as the main binary)
/// tokio::main();
/// ```
#[tokio::main]
async fn main() {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true)
        .with_level(true)
        .with_ansi(true)
        .with_timer(KoreanTime)
        .pretty()
        .init();

    info!("Starting queensac service...");

    // Load configuration
    let configuration = match get_configuration() {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to read configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Build application
    let app = match Application::build(configuration).await {
        Ok(app) => app,
        Err(e) => {
            error!("Failed to build application: {}", e);
            std::process::exit(1);
        }
    };

    // Create socket address
    let addr = SocketAddr::from(([127, 0, 0, 1], app.port));
    info!("Server listening on {}", addr);

    // Start the server
    let listener = match net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("Failed to bind to address {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    info!("queensac service is running on http://{}", addr);

    // Serve the application
    if let Err(e) = axum::serve(listener, app.router).await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }
}

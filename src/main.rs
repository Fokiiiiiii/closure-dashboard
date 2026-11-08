#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]

use closure_dashboard::{config::Config, router};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    closure_dashboard::serve::serve(listener, router(config), async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await;
    Ok(())
}

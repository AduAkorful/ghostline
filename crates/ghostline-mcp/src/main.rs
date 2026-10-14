// SPDX-License-Identifier: MIT

use ghostline_mcp::GhostlineMcp;
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("ghostline-mcp serving on stdio");
    let service = GhostlineMcp::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

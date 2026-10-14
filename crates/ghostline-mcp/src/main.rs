// SPDX-License-Identifier: MIT

use ghostline_mcp::{
    CallPaidArgs, ChannelArgs, SignArgs, VerifyArgs, call_paid, channel_id_hex, sign_voucher_hex,
    verify_voucher,
};
use rmcp::{
    ServerHandler, ServiceExt, handler::server::wrapper::Parameters, tool, tool_handler,
    tool_router, transport::stdio,
};
use serde::Serialize;

#[derive(Clone)]
struct GhostlineMcp {
    #[allow(dead_code)]
    tool_router: rmcp::handler::server::router::tool::ToolRouter<Self>,
}

#[derive(Serialize)]
struct HexOut {
    hex: String,
}

#[derive(Serialize)]
struct OkOut {
    ok: bool,
}

#[tool_router]
impl GhostlineMcp {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Compute the 32-byte Ghostline channel id from open parameters (hex).")]
    fn ghostline_channel_id(
        &self,
        Parameters(args): Parameters<ChannelArgs>,
    ) -> Result<String, String> {
        let hex = channel_id_hex(&args)?;
        serde_json::to_string(&HexOut { hex }).map_err(|e| e.to_string())
    }

    #[tool(
        description = "Sign a cumulative Ghostline voucher. Pass the payer secret as hex; do not log it."
    )]
    fn ghostline_sign_voucher(
        &self,
        Parameters(args): Parameters<SignArgs>,
    ) -> Result<String, String> {
        let hex = sign_voucher_hex(&args)?;
        serde_json::to_string(&HexOut { hex }).map_err(|e| e.to_string())
    }

    #[tool(description = "Verify a hex-encoded Ghostline voucher against channel parameters.")]
    fn ghostline_verify_voucher(
        &self,
        Parameters(args): Parameters<VerifyArgs>,
    ) -> Result<String, String> {
        verify_voucher(&args)?;
        serde_json::to_string(&OkOut { ok: true }).map_err(|e| e.to_string())
    }

    #[tool(
        description = "GET a paid HTTP resource. On 402, returns the Ghostline payment-required quote."
    )]
    async fn ghostline_call_paid(
        &self,
        Parameters(args): Parameters<CallPaidArgs>,
    ) -> Result<String, String> {
        let result = call_paid(&args).await?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }
}

#[tool_handler(
    name = "ghostline",
    version = "0.1.0",
    instructions = "Ghostline payment-channel tools. Signatures use the payer secret you pass; never echo it."
)]
impl ServerHandler for GhostlineMcp {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("ghostline-mcp serving on stdio");
    let service = GhostlineMcp::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

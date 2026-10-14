// SPDX-License-Identifier: MIT

use ghostline_mcp::{
    CallPaidArgs, ChannelArgs, SignArgs, TxHexArgs, VerifyArgs, call_paid, channel_id_hex,
    inspect_tx, sign_voucher_hex, unblind_tx, verify_voucher,
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
        description = "Sign the next Ghostline voucher for `price`. Uses GHOSTLINE_PAYER_SECRET, GHOSTLINE_SESSION, and policy env vars. Does not take a secret argument."
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
        description = "GET a paid HTTP resource. On 402, if GHOSTLINE_SESSION is set, signs a voucher from the session and retries once."
    )]
    async fn ghostline_call_paid(
        &self,
        Parameters(args): Parameters<CallPaidArgs>,
    ) -> Result<String, String> {
        let result = call_paid(&args).await?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }

    #[tool(
        description = "List stealth outputs in a sealed Ghostline tx hex (resource, tag, commitment, nonce, encrypted data). No secret."
    )]
    fn ghostline_inspect_tx(
        &self,
        Parameters(args): Parameters<TxHexArgs>,
    ) -> Result<String, String> {
        let views = inspect_tx(&args)?;
        serde_json::to_string(&views).map_err(|e| e.to_string())
    }

    #[tool(
        description = "Recover value and mask from stealth outputs in a sealed tx hex encrypted to GHOSTLINE_PAYER_SECRET. Does not take a secret argument."
    )]
    fn ghostline_unblind_tx(
        &self,
        Parameters(args): Parameters<TxHexArgs>,
    ) -> Result<String, String> {
        let found = unblind_tx(&args)?;
        serde_json::to_string(&found).map_err(|e| e.to_string())
    }
}

#[tool_handler(
    name = "ghostline",
    version = "0.1.0",
    instructions = "Ghostline payment-channel tools. Payer secret comes from GHOSTLINE_PAYER_SECRET, never from a tool argument."
)]
impl ServerHandler for GhostlineMcp {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("ghostline-mcp serving on stdio");
    let service = GhostlineMcp::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

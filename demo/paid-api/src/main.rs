// SPDX-License-Identifier: MIT

//! Demo server. Channel parameters are expected via env for a real run; tests construct the ledger in-process.

#[tokio::main]
async fn main() {
    eprintln!(
        "ghostline-paid-api is a library demo; run its tests with `cargo test -p ghostline-paid-api`"
    );
    eprintln!(
        "A bind address and funded channel are required before serving traffic. Not listening."
    );
}

// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetError {
    #[error("indexer URL is invalid: {0}")]
    Url(#[from] url::ParseError),
    #[error("failed to encode the transaction envelope")]
    Encode,
    #[error("indexer request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("indexer rejected {path}: HTTP {status}: {body}")]
    Rejected {
        path: String,
        status: u16,
        body: String,
    },
    #[error("indexer returned an unexpected body for {path}: {details}")]
    Decode { path: String, details: String },
}

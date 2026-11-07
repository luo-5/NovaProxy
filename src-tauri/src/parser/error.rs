use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Empty input")]
    EmptyInput,

    #[error("Unsupported protocol scheme: {0}")]
    UnsupportedScheme(String),

    #[error("Invalid URI format: {0}")]
    InvalidUri(String),

    #[error("Base64 decode error: {0}")]
    Base64(String),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Missing required field: {0}")]
    MissingField(&'static str),

    #[error("Invalid port: {0}")]
    InvalidPort(String),

    #[error("Network request failed: {0}")]
    Network(String),

    #[error("QR code error: {0}")]
    QrCode(String),
}

pub type ParseResult<T> = Result<T, ParseError>;

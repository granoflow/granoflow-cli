use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("usage error: {0}")]
    Usage(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("api error: {0}")]
    Api(String),
    #[error("auth error: {0}")]
    Auth(String),
    #[error("unsupported feature: {0}")]
    UnsupportedFeature(String),
    #[error("api gap: {0}")]
    ApiGap(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl CliError {
    pub fn code(&self) -> &'static str {
        match self {
            CliError::Usage(_) => "usage_error",
            CliError::Config(_) => "config_error",
            CliError::Network(_) => "network_error",
            CliError::Api(_) => "api_error",
            CliError::Auth(_) => "auth_error",
            CliError::UnsupportedFeature(_) => "unsupported_feature",
            CliError::ApiGap(_) => "api_gap",
            CliError::Internal(_) => "internal_error",
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Usage(_) => 2,
            CliError::Config(_) => 3,
            CliError::Auth(_) => 4,
            CliError::Network(_) => 5,
            CliError::Api(_) => 6,
            CliError::UnsupportedFeature(_) | CliError::ApiGap(_) => 7,
            CliError::Internal(_) => 10,
        }
    }
}

pub type CliResult<T> = Result<T, CliError>;

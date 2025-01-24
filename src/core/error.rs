use thiserror::Error;

#[derive(Error, Debug)]
pub enum RTError {
    #[error("Storage error: {message}")]
    StorageError {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Container error: {message}")]
    ContainerError {
        message: String,
        container_id: String,
    },

    #[error("System error: {message}")]
    SystemError {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Unexpected error: {message}")]
    UnexpectedError {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl RTError {
    pub fn storage_error(
        message: &str,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        RTError::StorageError {
            message: message.to_string(),
            source,
        }
    }

    pub fn container_error(message: &str, container_id: &str) -> Self {
        RTError::ContainerError {
            message: message.to_string(),
            container_id: container_id.to_string(),
        }
    }

    pub fn unexpected_error(
        message: &str,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        RTError::UnexpectedError {
            message: message.to_string(),
            source,
        }
    }
}

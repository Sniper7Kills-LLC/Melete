use thiserror::Error;

#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("failed to parse template TOML: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("failed to serialize template TOML: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("unsupported template schema version: {0}")]
    SchemaVersion(u32),

    #[error("invalid uuid: {0}")]
    InvalidUuid(String),
}

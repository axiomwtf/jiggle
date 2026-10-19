use thiserror::Error;

#[derive(Error, Debug)]
pub enum JglcError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Compiler error: {0}")]
    Compiler(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Invalid block syntax: {0}")]
    BlockSyntax(String),

    #[error("Unknown language: {0}")]
    UnknownLanguage(String),

    #[error("Custom language without dep block: {0}")]
    CustomLanguageMissingDep(String),

    #[error("Import error: {0}")]
    Import(String),

    #[error("Dependency version conflict: {0}")]
    DepsConflict(String),

    #[error("Generated dep file name collision: {0}")]
    DepFileNameCollision(String),

    #[error("Tar archive error: {0}")]
    Tar(String),

    #[error("XZ compression error: {0}")]
    Xz(String),

    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

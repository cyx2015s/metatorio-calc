#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    ContextCreationError(String),
    SolverError(String),
    RegistryError(String),
    Custom(String),
}

impl<T: ToString> From<T> for AppError {
    fn from(err: T) -> Self {
        AppError::Custom(err.to_string())
    }
}

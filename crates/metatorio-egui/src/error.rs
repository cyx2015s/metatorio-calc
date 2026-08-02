use std::fmt::Display;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    None,
    UpToDate,
    RestartRequired,
    ContextCreation(String),
    Solver(String),
    Registry(String),
    Update(String),
    Network(String),
    Io(String),
    Custom(String),
}

impl<T: Display> From<T> for AppError {
    fn from(err: T) -> Self {
        AppError::Custom(err.to_string())
    }
}

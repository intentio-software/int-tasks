#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("a task needs a title")]
    EmptyTitle,

    #[error("no task with id {0}")]
    TaskNotFound(String),

    #[error("no list with id {0}")]
    ListNotFound(String),

    #[error("no board with id {0}")]
    BoardNotFound(String),

    #[error("no session with id {0}")]
    SessionNotFound(String),

    #[error("the store has no boards to put a task in")]
    NoBoards,

    /// The store exists but cannot be parsed. Reported rather than silently
    /// re-seeded, because quietly replacing someone's tasks with an empty store
    /// is worse than refusing to start.
    #[error("the task store is unreadable: {0}")]
    Corrupt(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, TaskError>;

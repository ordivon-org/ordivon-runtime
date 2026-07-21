mod error;
mod file;
mod repo;
mod search;

pub use error::{ExecError, ExecErrorCode};
pub use file::{
    read_many, read_text, ReadManyItem, ReadManyRequest, ReadManyResult, ReadTextRequest,
    ReadTextResult, MAX_BATCH_FILES, MAX_READ_BYTES, MAX_READ_LINES,
};
pub use repo::{repo_snapshot, RepoSnapshotRequest, RepoSnapshotResult};
pub use search::{
    search_text, SearchHit, SearchPatternMode, SearchSubmatch, SearchTextRequest, SearchTextResult,
    MAX_SEARCH_BYTES, MAX_SEARCH_GLOBS, MAX_SEARCH_RESULTS,
};

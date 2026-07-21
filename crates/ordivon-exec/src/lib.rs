mod error;
mod file;

pub use error::{ExecError, ExecErrorCode};
pub use file::{
    read_many, read_text, ReadManyItem, ReadManyRequest, ReadManyResult, ReadTextRequest,
    ReadTextResult, MAX_BATCH_FILES, MAX_READ_BYTES, MAX_READ_LINES,
};

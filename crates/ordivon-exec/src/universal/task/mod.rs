mod artifact;
mod compact;
mod start;
mod status;

pub use artifact::read_task_artifact;
pub use compact::{await_universal_task_compact, run_universal_task_compact};
pub use start::start_universal_task;
pub use status::{cancel_universal_task, get_universal_task};

pub(super) const RESULT_FILE: &str = "result.json";
pub(super) const REQUEST_FILE: &str = "request.json";
pub(super) const METADATA_FILE: &str = "metadata.json";
pub(super) const CANCEL_FILE: &str = "cancel-requested.json";
pub(super) const STDOUT_FILE: &str = "stdout.log";
pub(super) const STDERR_FILE: &str = "stderr.log";

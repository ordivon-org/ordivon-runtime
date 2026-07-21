use ordivon_exec::run_task_runner;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(flag) = args.next() else {
        eprintln!("usage: ordivon-task-runner --task-dir <absolute-path>");
        return ExitCode::from(64);
    };
    if flag != "--task-dir" {
        eprintln!("unknown argument: {flag}");
        return ExitCode::from(64);
    }
    let Some(task_dir) = args.next() else {
        eprintln!("--task-dir requires a value");
        return ExitCode::from(64);
    };
    if args.next().is_some() {
        eprintln!("unexpected additional arguments");
        return ExitCode::from(64);
    }
    match run_task_runner(&PathBuf::from(task_dir)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("runner error: {error}");
            ExitCode::from(70)
        }
    }
}

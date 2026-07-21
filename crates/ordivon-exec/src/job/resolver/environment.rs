pub(super) fn is_forbidden_base_environment(name: &str) -> bool {
    matches!(
        name,
        "LD_PRELOAD"
            | "LD_LIBRARY_PATH"
            | "BASH_ENV"
            | "ENV"
            | "SHELLOPTS"
            | "PS4"
            | "PYTHONPATH"
            | "PYTHONHOME"
            | "NODE_OPTIONS"
            | "RUBYOPT"
            | "PERL5OPT"
            | "RUSTC_WRAPPER"
            | "RUSTC_WORKSPACE_WRAPPER"
            | "CARGO_BUILD_RUSTC_WRAPPER"
            | "GIT_SSH_COMMAND"
            | "GIT_CONFIG_COUNT"
            | "SSH_ASKPASS"
    ) || name.starts_with("DYLD_")
        || name.starts_with("GIT_CONFIG_KEY_")
        || name.starts_with("GIT_CONFIG_VALUE_")
        || (name.starts_with("CARGO_TARGET_") && name.ends_with("_RUNNER"))
}

pub(super) fn is_forbidden_client_override(name: &str) -> bool {
    is_forbidden_base_environment(name)
        || matches!(
            name,
            "PATH"
                | "HOME"
                | "CARGO_HOME"
                | "RUSTUP_HOME"
                | "CARGO_TARGET_DIR"
                | "TMPDIR"
                | "TMP"
                | "TEMP"
        )
}

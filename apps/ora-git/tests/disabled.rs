use pretty_assertions::assert_eq;
use std::process::Command;

/// Agents get the Node's specific refusal, even if a stale socket variable was inherited.
#[test]
fn disabled_reason_is_an_explicit_cli_error() -> Result<(), Box<dyn std::error::Error>> {
    for reason in [
        "ora-git is disabled: process.workload_uid is not set; it must differ from the Node effective UID",
        "ora-git is disabled: process.workload_uid (1000) equals the Node effective UID; configure a separate workload UID",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_ora-git"))
            .arg("info")
            .env(ora_git::DISABLED_ENV, reason)
            .env(ora_git::SOCKET_ENV, "missing.sock")
            .output()?;
        assert_eq!(
            (
                output.status.code(),
                output.stdout,
                String::from_utf8(output.stderr)?
            ),
            (
                Some(10),
                Vec::new(),
                format!("ora-git: unavailable: {reason}\n")
            ),
        );
    }
    Ok(())
}

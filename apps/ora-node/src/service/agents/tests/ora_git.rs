use crate::ora_git::Availability;
use crate::{Node, Shutdown};
use pretty_assertions::assert_eq;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

/// Composes the production service with clone and Git Server access configured.
fn compose(
    workload_uid: Option<u32>,
) -> Result<(tempfile::TempDir, Availability), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let (mut deployment, clone) = super::restored_deployment(root.path())?;
    // SAFETY: geteuid only reads this process's identity.
    deployment.process.expected_uid = unsafe { libc::geteuid() };
    deployment.process.workload_uid = workload_uid;
    let mut node = Node::open(deployment.node, deployment.process, Shutdown::default())?;
    node.configure_clone(clone)?;
    node.configure_git_access(crate::GitAccessConfig {
        ca_cert: root.path().join("ca.pem"),
        client_cert: root.path().join("client.pem"),
        client_key: root.path().join("client-key.pem"),
    });
    let service = super::super::ora_git(
        &node,
        &root.path().join("sockets"),
        node.database.session_journal()?,
        Shutdown::default(),
    )?
    .ok_or("ora-git configuration was ignored")?;
    Ok((root, service))
}

/// Missing configuration must not create the service under the Node identity.
#[test]
fn ora_git_is_disabled_without_workload_uid() -> Result<(), Box<dyn std::error::Error>> {
    ora_logging::initialize_test_clock();
    ora_logging::with_trace_logging(|| {
        let (root, service) = compose(/*workload_uid*/ None)?;
        let Availability::Disabled(reason) = service else {
            panic!("ora-git was enabled")
        };
        assert_eq!(
            reason,
            "ora-git is disabled: process.workload_uid is not set; it must differ from the Node effective UID"
        );
        assert!(!root.path().join("sockets").exists());
        Ok(())
    })
}

/// Explicitly configuring the Node's UID is just as unsafe as omitting it.
#[test]
fn ora_git_is_disabled_for_the_node_uid() -> Result<(), Box<dyn std::error::Error>> {
    ora_logging::initialize_test_clock();
    ora_logging::with_trace_logging(|| {
        // SAFETY: geteuid only reads this process's identity.
        let uid = unsafe { libc::geteuid() };
        let service = Availability::for_workload(Some(uid), uid, |_| panic!("same UID enabled"))?;
        let Availability::Disabled(reason) = service else {
            panic!("ora-git was enabled")
        };
        assert_eq!(
            reason,
            format!(
                "ora-git is disabled: process.workload_uid ({uid}) equals the Node effective UID; configure a separate workload UID"
            )
        );
        Ok(())
    })
}

/// A separate UID reaches service construction unchanged, regardless of the test runner's UID.
#[test]
fn ora_git_keeps_separate_workload_uid() -> Result<(), Box<dyn std::error::Error>> {
    ora_logging::initialize_test_clock();
    ora_logging::with_trace_logging(|| {
        // SAFETY: geteuid only reads this process's identity.
        let uid = unsafe { libc::geteuid() };
        let service = Availability::for_workload(Some(uid ^ 1), uid, |peer_uid| {
            assert_eq!(peer_uid, uid ^ 1);
            Ok(Arc::new(UnopenedBroker))
        })?;
        assert!(matches!(service, Availability::Enabled(_)));
        Ok(())
    })
}

/// Identity selection must not need to open a session socket.
struct UnopenedBroker;

impl crate::ora_git::OraGitBroker for UnopenedBroker {
    /// This test exercises selection, not socket serving.
    fn open(&self, _: crate::ora_git::SessionGit) -> io::Result<crate::ora_git::OraGitSocket> {
        panic!("identity selection opened a session")
    }
}

/// A test-local writer records the same wrapper event startup emits.
#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl Write for Logs {
    /// Appends one formatted event to this test's buffer.
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("log buffer poisoned"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    /// The in-memory writer has no pending output.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Both refusal paths explain the config field in the startup log.
#[test]
fn ora_git_startup_logs_the_disabled_reason() -> Result<(), Box<dyn std::error::Error>> {
    ora_logging::initialize_test_clock();
    let logs = Logs::default();
    let writer = logs.clone();
    ora_logging::with_recorded_trace_logging(
        tracing_subscriber::fmt::layer()
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone()),
        || -> Result<(), Box<dyn std::error::Error>> {
            drop(compose(/*workload_uid*/ None)?);
            // SAFETY: geteuid only reads this process's identity.
            let uid = unsafe { libc::geteuid() };
            drop(Availability::for_workload(Some(uid), uid, |_| {
                panic!("same UID enabled")
            })?);
            Ok(())
        },
    )?;
    let text = String::from_utf8(
        logs.0
            .lock()
            .map_err(|_| io::Error::other("log buffer poisoned"))?
            .clone(),
    )?;
    assert!(text.contains("ora-git disabled"), "{text}");
    assert!(text.contains("process.workload_uid is not set"), "{text}");
    assert!(text.contains("equals the Node effective UID"), "{text}");
    Ok(())
}

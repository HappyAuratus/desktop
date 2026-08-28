use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::{Mutex, PoisonError};

use crate::tree::ReaperTargets;

const READY_FRAME: &[u8; 4] = b"ORA1";
const REGISTER: u8 = 1;
const UNREGISTER: u8 = 2;
const SHUTDOWN: u8 = 3;
const ACKNOWLEDGED: u8 = 0;
const FAILED: u8 = 1;

static REAPER: Mutex<ReaperState> = Mutex::new(ReaperState::Uninitialized);

enum ReaperState {
    Uninitialized,
    Running(ReaperClient),
    Stopped,
}

/// Starts the process reaper used by every subsequently created [`crate::TokioProcessSpawner`].
///
/// Initialization is process-global because all production spawners belong to one Desktop
/// process and must share the same parent-liveness pipe. Repeated initialization is idempotent.
pub fn initialize_reaper(program: impl AsRef<Path>) -> io::Result<()> {
    let mut reaper = REAPER.lock().unwrap_or_else(PoisonError::into_inner);
    match &*reaper {
        ReaperState::Uninitialized => {
            *reaper = ReaperState::Running(ReaperClient::spawn(program.as_ref())?);
            Ok(())
        }
        ReaperState::Running(_) => Ok(()),
        ReaperState::Stopped => Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "process reaper cannot restart after shutdown",
        )),
    }
}

/// Requests final cleanup, waits for its acknowledgement, and reaps the reaper sidecar.
///
/// Desktop calls this only after its normal application lifecycle has released the owners that
/// get an opportunity to stop their children gracefully. Abnormal parent termination cannot run
/// this function; closing the IPC pipe makes the sidecar perform the same cleanup instead.
pub fn shutdown_reaper() -> io::Result<()> {
    let previous = {
        let mut reaper = REAPER.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::replace(&mut *reaper, ReaperState::Stopped)
    };
    match previous {
        ReaperState::Running(client) => client.shutdown(),
        ReaperState::Uninitialized | ReaperState::Stopped => Ok(()),
    }
}

/// Runs the reaper protocol on standard input and output until its Ora parent shuts down.
///
/// This is the complete entry point for the version-locked `ora-reaper` sidecar. EOF is treated
/// exactly like an explicit shutdown, so normal exit and crashes converge on the same cleanup.
pub fn run_reaper() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    run_reaper_io(stdin.lock(), stdout.lock())
}

/// Registers a process tree when Desktop installed a process-global reaper.
pub(crate) fn register_process(process_id: u32) -> io::Result<()> {
    let mut reaper = REAPER.lock().unwrap_or_else(PoisonError::into_inner);
    match &mut *reaper {
        ReaperState::Running(reaper) => reaper.request(REGISTER, process_id),
        ReaperState::Uninitialized => Ok(()),
        ReaperState::Stopped => Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "cannot spawn a process after the process reaper stopped",
        )),
    }
}

/// Removes a tree whose direct child has exited so stale process identifiers cannot be reused.
pub(crate) fn unregister_process(process_id: u32) -> io::Result<()> {
    let mut reaper = REAPER.lock().unwrap_or_else(PoisonError::into_inner);
    match &mut *reaper {
        ReaperState::Running(reaper) => reaper.request(UNREGISTER, process_id),
        ReaperState::Uninitialized | ReaperState::Stopped => Ok(()),
    }
}

struct ReaperClient {
    child: Child,
    writer: BufWriter<ChildStdin>,
    reader: BufReader<ChildStdout>,
}

impl ReaperClient {
    /// Starts one sidecar and waits for readiness before exposing the connection to spawners.
    fn spawn(program: &Path) -> io::Result<Self> {
        let mut command = std::process::Command::new(program);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        configure_reaper_command(&mut command);

        let mut child = command.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("spawned process reaper has no stdin pipe"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("spawned process reaper has no stdout pipe"))?;
        let mut client = Self {
            child,
            writer: BufWriter::new(stdin),
            reader: BufReader::new(stdout),
        };
        let mut ready = [0_u8; READY_FRAME.len()];
        client.reader.read_exact(&mut ready)?;
        if &ready != READY_FRAME {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "process reaper returned an invalid readiness frame",
            ));
        }
        Ok(client)
    }

    /// Sends one serialized request and verifies that the sidecar handled the same operation.
    fn request(&mut self, operation: u8, process_id: u32) -> io::Result<()> {
        self.writer.write_all(&[operation])?;
        self.writer.write_all(&process_id.to_le_bytes())?;
        self.writer.flush()?;

        let mut response = [0_u8; 6];
        self.reader.read_exact(&mut response)?;
        let acknowledged_process_id = u32::from_le_bytes(
            response[2..]
                .try_into()
                .map_err(|_| io::Error::other("invalid process reaper response length"))?,
        );
        if response[0] != ACKNOWLEDGED
            || response[1] != operation
            || acknowledged_process_id != process_id
        {
            return Err(io::Error::other(format!(
                "process reaper rejected operation {operation} for pid {process_id}"
            )));
        }
        Ok(())
    }

    /// Performs final cleanup before waiting for the sidecar's own process exit.
    fn shutdown(mut self) -> io::Result<()> {
        let request_result = self.request(SHUTDOWN, /*process_id*/ 0);
        // Closing stdin also preserves the EOF fallback if the explicit request failed halfway.
        drop(self.writer);
        let wait_result = self.child.wait();
        request_result?;
        let status = wait_result?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "process reaper exited unsuccessfully: {status}"
            )))
        }
    }
}

/// Places the monitor outside Ora's process group so it survives long enough to observe EOF.
fn configure_reaper_command(command: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = command;
    }
}

/// Serves the local protocol and converges explicit shutdown and parent EOF on one cleanup path.
fn run_reaper_io(mut reader: impl Read, mut writer: impl Write) -> io::Result<()> {
    let mut targets = ReaperTargets::new()?;
    writer.write_all(READY_FRAME)?;
    writer.flush()?;

    loop {
        let Some((operation, process_id)) = read_request(&mut reader)? else {
            return targets.kill_all();
        };
        let result = match operation {
            REGISTER => targets.register(process_id),
            UNREGISTER => targets.unregister(process_id),
            SHUTDOWN => targets.kill_all(),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown process reaper operation {operation}"),
            )),
        };
        write_response(&mut writer, operation, process_id, &result)?;
        result?;
        if operation == SHUTDOWN {
            return Ok(());
        }
    }
}

/// Reads one fixed-width request while distinguishing clean parent EOF from truncation.
fn read_request(reader: &mut impl Read) -> io::Result<Option<(u8, u32)>> {
    let mut frame = [0_u8; 5];
    let mut read_bytes = 0;
    while read_bytes < frame.len() {
        match reader.read(&mut frame[read_bytes..])? {
            0 if read_bytes == 0 => return Ok(None),
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "process reaper received a truncated request",
                ));
            }
            count => read_bytes += count,
        }
    }
    Ok(Some((
        frame[0],
        u32::from_le_bytes(
            frame[1..]
                .try_into()
                .map_err(|_| io::Error::other("invalid process reaper request length"))?,
        ),
    )))
}

/// Writes the compact response used by the synchronous parent-side registration seam.
fn write_response(
    writer: &mut impl Write,
    operation: u8,
    process_id: u32,
    result: &io::Result<()>,
) -> io::Result<()> {
    writer.write_all(&[if result.is_ok() { ACKNOWLEDGED } else { FAILED }])?;
    writer.write_all(&[operation])?;
    writer.write_all(&process_id.to_le_bytes())?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::{ACKNOWLEDGED, READY_FRAME, REGISTER, SHUTDOWN, UNREGISTER, run_reaper_io};
    use pretty_assertions::assert_eq;

    /// Verifies duplicate, unregistered, and already-gone targets make cleanup idempotent.
    #[test]
    fn protocol_is_idempotent_for_gone_processes() {
        let missing_process_id = u32::MAX;
        let requests = [
            request(REGISTER, missing_process_id),
            request(REGISTER, missing_process_id),
            request(UNREGISTER, missing_process_id),
            request(UNREGISTER, missing_process_id),
            request(SHUTDOWN, 0),
        ]
        .concat();
        let mut output = Vec::new();

        run_reaper_io(requests.as_slice(), &mut output)
            .unwrap_or_else(|error| panic!("reap missing process: {error}"));

        let expected = [
            READY_FRAME.to_vec(),
            response(REGISTER, missing_process_id),
            response(REGISTER, missing_process_id),
            response(UNREGISTER, missing_process_id),
            response(UNREGISTER, missing_process_id),
            response(SHUTDOWN, 0),
        ]
        .concat();
        assert_eq!(output, expected);
    }

    /// Verifies a partial registration cannot be mistaken for a valid process identifier.
    #[test]
    fn rejects_truncated_request() {
        let mut output = Vec::new();
        let error = match run_reaper_io([REGISTER, 1, 2].as_slice(), &mut output) {
            Ok(()) => panic!("truncated request must fail"),
            Err(error) => error,
        };

        assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof);
    }

    fn request(operation: u8, process_id: u32) -> Vec<u8> {
        [vec![operation], process_id.to_le_bytes().to_vec()].concat()
    }

    fn response(operation: u8, process_id: u32) -> Vec<u8> {
        [
            vec![ACKNOWLEDGED, operation],
            process_id.to_le_bytes().to_vec(),
        ]
        .concat()
    }
}

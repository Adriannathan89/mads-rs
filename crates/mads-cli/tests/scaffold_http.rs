//! Live HTTP evidence for the generated minimal MADS application.

#![cfg(target_os = "linux")]

use std::{
    fs,
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use tempfile::tempdir;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[test]
fn generated_application_serves_hello_world_on_the_overridden_localhost_port() {
    let invocation = tempdir().expect("temporary invocation directory should be created");
    let generation = Command::new(env!("CARGO_BIN_EXE_mads"))
        .current_dir(invocation.path())
        .args(["new", "generated-http-app"])
        .output()
        .expect("mads new should run");
    assert_success("mads new", &generation);

    let project = invocation.path().join("generated-http-app");
    substitute_local_mads(&project);
    let address = reserve_localhost_address().expect("localhost should provide an available port");

    let mut command = Command::new(env!("CARGO_BIN_EXE_mads"));
    command
        .current_dir(&project)
        .args(["run"])
        .env("CARGO_NET_OFFLINE", "true")
        .env("MADS_SERVER__HOST", "127.0.0.1")
        .env("MADS_SERVER__PORT", address.port().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = ChildGuard::new(command.spawn().expect("mads run should start"));

    let response = wait_for_http_response(&mut child, address);
    let (status, body) = response
        .split_once("\r\n\r\n")
        .expect("HTTP response should contain a header/body separator");
    assert!(
        status.starts_with("HTTP/1.1 200 "),
        "GET / should return HTTP 200; response={response:?}"
    );
    assert_eq!(body, "Hello World!");
}

fn substitute_local_mads(project: &Path) {
    let manifest_path = project.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path).expect("generated manifest should exist");
    let local_mads = workspace_root()
        .join("crates/mads")
        .canonicalize()
        .expect("local mads crate should exist");
    let local_path = local_mads.to_string_lossy().replace('\\', "\\\\");
    let registry_dependency = format!(
        "mads = {{ version = \"={}\", default-features = false, features = [\"http\", \"runtime-tokio\"] }}",
        env!("CARGO_PKG_VERSION")
    );
    let local_dependency = format!(
        "mads = {{ path = \"{local_path}\", version = \"={}\", default-features = false, features = [\"http\", \"runtime-tokio\"] }}",
        env!("CARGO_PKG_VERSION")
    );
    let substituted = manifest.replacen(&registry_dependency, &local_dependency, 1);
    assert_ne!(
        substituted, manifest,
        "the registry MADS dependency should be replaced exactly once"
    );
    assert!(
        !substituted.contains(&registry_dependency),
        "the generated manifest must not retain a registry MADS dependency after substitution"
    );
    fs::write(manifest_path, substituted).expect("test-only local substitution should succeed");
}

fn reserve_localhost_address() -> io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    Ok(address)
}

fn wait_for_http_response(child: &mut ChildGuard, address: SocketAddr) -> String {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    loop {
        if let Some(status) = child
            .try_wait()
            .expect("mads run status should be readable while waiting for startup")
        {
            child.panic_with_output(
                "mads run exited before the generated application was ready",
                status,
            );
        }

        if let Some(response) = get_root(address) {
            return response;
        }

        assert!(
            Instant::now() < deadline,
            "timed out waiting for generated application at {address}"
        );
        thread::sleep(POLL_INTERVAL);
    }
}

fn get_root(address: SocketAddr) -> Option<String> {
    let mut stream = TcpStream::connect_timeout(&address, POLL_INTERVAL).ok()?;
    stream.set_read_timeout(Some(POLL_INTERVAL)).ok()?;
    stream.set_write_timeout(Some(POLL_INTERVAL)).ok()?;
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    Some(response)
}

struct ChildGuard {
    child: Option<Child>,
    process_group: u32,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self {
            process_group: child.id(),
            child: Some(child),
        }
    }

    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        self.child
            .as_mut()
            .expect("mads run child should remain owned by its guard")
            .try_wait()
    }

    fn panic_with_output(&mut self, context: &str, status: std::process::ExitStatus) -> ! {
        let output = self
            .child
            .take()
            .expect("exited mads run child should remain owned by its guard")
            .wait_with_output()
            .expect("exited mads run output should be readable");
        panic!(
            "{context} with {status}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn terminate_and_wait(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = Command::new("kill")
                .args(["-TERM", "--", &format!("-{}", self.process_group)])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            let _ = child.wait();
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.terminate_and_wait();
    }
}

fn assert_success(label: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("mads-cli should live below the workspace crates directory")
        .to_path_buf()
}

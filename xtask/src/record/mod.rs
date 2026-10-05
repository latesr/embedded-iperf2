//! `cargo xtask record`: runs a stock iperf 2.2.2 client and server on loopback, through a relay that writes the
//! bytes of each direction to a sample file.

pub mod sample;

use sample::Direction;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// The version the samples come from. `MCU_IPERF2` must name a binary that reports it.
const VERSION: &str = "2.2.2";

/// One recorded case: its file name, whether it runs over UDP, and the client's arguments after `-c` and `-p`.
struct Case {
    name: &'static str,
    udp: bool,
    args: &'static [&'static str],
}

const CASES: &[Case] = &[
    Case {
        name: "tcp",
        udp: false,
        args: &["-n", "64K", "-l", "16K"],
    },
    Case {
        name: "tcp-reverse",
        udp: false,
        args: &["-n", "64K", "-l", "16K", "-R"],
    },
    Case {
        name: "tcp-time",
        udp: false,
        args: &["-t", "2", "-i", "1", "-l", "16K", "-b", "256K"],
    },
    Case {
        name: "udp",
        udp: true,
        args: &["-t", "2", "-i", "1", "-l", "512", "-b", "256K"],
    },
    Case {
        name: "udp-reverse",
        udp: true,
        args: &["-t", "2", "-i", "1", "-l", "512", "-b", "256K", "-R"],
    },
    Case {
        name: "udp-bytes",
        udp: true,
        args: &["-n", "16K", "-l", "512", "-b", "256K"],
    },
];

/// The time the server gets to open its socket before the client starts.
const SERVER_START: Duration = Duration::from_millis(500);
/// The time the relay keeps forwarding after the client exits, for datagrams still on the way.
const UDP_DRAIN: Duration = Duration::from_millis(500);
/// The longest wait of one UDP receive, so the relay sees the stop flag.
const UDP_POLL: Duration = Duration::from_millis(100);

/// Records every case, or only the cases named in `only`, into the crate's `tests/samples/<version>/`.
pub fn run(root: &Path, only: &[String]) -> Result<(), String> {
    let binary = std::env::var("MCU_IPERF2")
        .map_err(|_| "MCU_IPERF2 must name an iperf 2.2.2 binary".to_owned())?;
    let version = version_line(&binary)?;
    if !version.starts_with(&format!("iperf {VERSION} ")) {
        return Err(format!("{binary} reports `{version}`, not {VERSION}"));
    }
    if let Some(unknown) = only
        .iter()
        .find(|name| !CASES.iter().any(|case| case.name == name.as_str()))
    {
        return Err(format!("no case is named {unknown}"));
    }
    let folder = root.join("embedded-iperf2/tests/samples").join(VERSION);
    std::fs::create_dir_all(&folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    for case in CASES
        .iter()
        .filter(|case| only.is_empty() || only.iter().any(|name| name == case.name))
    {
        println!("==> {}", case.name);
        let recording = record(&binary, case)?;
        let write = |extension: &str, bytes: &[u8]| {
            let path = folder.join(format!("{}.{extension}", case.name));
            std::fs::write(&path, bytes).map_err(|error| format!("{}: {error}", path.display()))
        };
        write("bin", &recording.sample)?;
        write("client.txt", &recording.client)?;
        write("server.txt", &recording.server)?;
    }
    let readme = folder.join("README.md");
    std::fs::write(&readme, index(&version))
        .map_err(|error| format!("{}: {error}", readme.display()))?;
    println!("==> recorded with {version}");
    Ok(())
}

/// The `README.md` that lists the cases and how each was run.
fn index(version: &str) -> String {
    let mut text = format!(
        "# iperf {VERSION} samples\n\n\
         Covers: the samples that `cargo xtask record` recorded from `{version}`.\n\n\
         Each case has three files. `<case>.bin` holds the bytes the relay saw, and the rustdoc of \
         `xtask/src/record/sample.rs` states its layout. `<case>.client.txt` and `<case>.server.txt` hold what the \
         client and the server printed. The server ran as `iperf -s -p <port>`, with `-u` for UDP. The client ran as \
         `iperf -c 127.0.0.1 -p <relay port>`, with `-u` for UDP, and then the arguments in the table.\n\n\
         | Case | Transport | Client arguments |\n|---|---|---|\n"
    );
    for case in CASES {
        let transport = if case.udp { "UDP" } else { "TCP" };
        text.push_str(&format!(
            "| `{}` | {transport} | `{}` |\n",
            case.name,
            case.args.join(" ")
        ));
    }
    text
}

/// The first line that `binary -v` prints. iperf 2 prints it on standard error.
fn version_line(binary: &str) -> Result<String, String> {
    let output = Command::new(binary)
        .arg("-v")
        .output()
        .map_err(|error| format!("{binary}: {error}"))?;
    let text = [output.stdout, output.stderr].concat();
    Ok(String::from_utf8_lossy(&text)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned())
}

struct Recording {
    sample: Vec<u8>,
    client: Vec<u8>,
    server: Vec<u8>,
}

fn record(binary: &str, case: &Case) -> Result<Recording, String> {
    let server_port = free_port(case.udp)?;
    let mut server_command = Command::new(binary);
    server_command.args(["-s", "-p", &server_port.to_string()]);
    if case.udp {
        server_command.arg("-u");
    }
    let server = server_command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{binary}: {error}"))?;
    let server = KillOnDrop(Some(server));
    thread::sleep(SERVER_START);

    let server_address = SocketAddr::from(([127, 0, 0, 1], server_port));
    let (sample, client) = if case.udp {
        relay_udp(binary, case, server_address)?
    } else {
        relay_tcp(binary, case, server_address)?
    };
    let server = server.interrupt()?;
    Ok(Recording {
        sample,
        client: [client.stdout, client.stderr].concat(),
        server: [server.stdout, server.stderr].concat(),
    })
}

/// A port on 127.0.0.1 that was free a moment ago.
fn free_port(udp: bool) -> Result<u16, String> {
    let address = if udp {
        UdpSocket::bind("127.0.0.1:0").and_then(|socket| socket.local_addr())
    } else {
        TcpListener::bind("127.0.0.1:0").and_then(|listener| listener.local_addr())
    };
    address
        .map(|address| address.port())
        .map_err(|error| format!("no free port: {error}"))
}

/// Starts the client against `port` on 127.0.0.1 and waits for it to exit.
fn run_client(binary: &str, case: &Case, port: u16, udp: bool) -> Result<Output, String> {
    let mut command = Command::new(binary);
    command.args(["-c", "127.0.0.1", "-p", &port.to_string()]);
    if udp {
        command.arg("-u");
    }
    let output = command
        .args(case.args)
        .output()
        .map_err(|error| format!("{binary}: {error}"))?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "the client failed: {}",
            String::from_utf8_lossy(&[output.stdout, output.stderr].concat())
        ))
    }
}

/// Relays one TCP connection from the client to the server, and fails when the client opens a second one.
fn relay_tcp(binary: &str, case: &Case, server: SocketAddr) -> Result<(Vec<u8>, Output), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let sample = Arc::new(Mutex::new(Vec::new()));
    let relay = {
        let sample = Arc::clone(&sample);
        thread::spawn(move || -> Result<TcpListener, String> {
            let (client, _) = listener.accept().map_err(|error| error.to_string())?;
            let upstream = TcpStream::connect(server).map_err(|error| error.to_string())?;
            let copies = [
                (&client, &upstream, Direction::ClientToServer),
                (&upstream, &client, Direction::ServerToClient),
            ]
            .map(|(from, to, direction)| {
                let from = from.try_clone().map_err(|error| error.to_string());
                let to = to.try_clone().map_err(|error| error.to_string());
                let sample = Arc::clone(&sample);
                thread::spawn(move || copy_tcp(from?, to?, direction, &sample))
            });
            for copy in copies {
                copy.join()
                    .map_err(|_| "a relay thread panicked".to_owned())??;
            }
            Ok(listener)
        })
    };
    let client = run_client(binary, case, port, false)?;
    let listener = relay
        .join()
        .map_err(|_| "the relay panicked".to_owned())??;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    if listener.accept().is_ok() {
        return Err("the client opened a second connection".to_owned());
    }
    Ok((take(&sample)?, client))
}

/// Copies `from` to `to` until `from` ends, and records each read.
fn copy_tcp(
    mut from: TcpStream,
    mut to: TcpStream,
    direction: Direction,
    sample: &Mutex<Vec<u8>>,
) -> Result<(), String> {
    let mut buffer = vec![0; 65536];
    loop {
        let read = from.read(&mut buffer).map_err(|error| error.to_string())?;
        let bytes = buffer.get(..read).unwrap_or_default();
        if bytes.is_empty() {
            // The other side may still be sending, so only this direction ends.
            return to
                .shutdown(Shutdown::Write)
                .or_else(|error| match error.kind() {
                    ErrorKind::NotConnected => Ok(()),
                    _ => Err(error.to_string()),
                });
        }
        append(sample, direction, bytes)?;
        to.write_all(bytes).map_err(|error| error.to_string())?;
    }
}

/// Relays the datagrams between the client and the server until the client exits and the last ones arrive.
fn relay_udp(binary: &str, case: &Case, server: SocketAddr) -> Result<(Vec<u8>, Output), String> {
    let front = UdpSocket::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    let port = front
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let back = UdpSocket::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    back.connect(server).map_err(|error| error.to_string())?;
    for socket in [&front, &back] {
        socket
            .set_read_timeout(Some(UDP_POLL))
            .map_err(|error| error.to_string())?;
    }
    let sample = Arc::new(Mutex::new(Vec::new()));
    let client_address = Arc::new(Mutex::new(None));
    let stop = Arc::new(AtomicBool::new(false));

    let forward = {
        let (front, back) = clone_pair(&front, &back)?;
        let (sample, client_address, stop) = (
            Arc::clone(&sample),
            Arc::clone(&client_address),
            Arc::clone(&stop),
        );
        thread::spawn(move || -> Result<(), String> {
            let mut buffer = vec![0; 65536];
            while !stop.load(Ordering::Relaxed) {
                let (read, from) = match front.recv_from(&mut buffer) {
                    Ok(received) => received,
                    Err(error) if timed_out(&error) => continue,
                    Err(error) => return Err(error.to_string()),
                };
                *client_address.lock().map_err(|_| "poisoned".to_owned())? = Some(from);
                let bytes = buffer.get(..read).unwrap_or_default();
                append(&sample, Direction::ClientToServer, bytes)?;
                back.send(bytes).map_err(|error| error.to_string())?;
            }
            Ok(())
        })
    };
    let backward = {
        let (front, back) = clone_pair(&front, &back)?;
        let (sample, client_address, stop) = (
            Arc::clone(&sample),
            Arc::clone(&client_address),
            Arc::clone(&stop),
        );
        thread::spawn(move || -> Result<(), String> {
            let mut buffer = vec![0; 65536];
            while !stop.load(Ordering::Relaxed) {
                let read = match back.recv(&mut buffer) {
                    Ok(read) => read,
                    Err(error) if timed_out(&error) => continue,
                    Err(error) => return Err(error.to_string()),
                };
                let bytes = buffer.get(..read).unwrap_or_default();
                append(&sample, Direction::ServerToClient, bytes)?;
                let to = *client_address.lock().map_err(|_| "poisoned".to_owned())?;
                let to = to.ok_or("the server sent before the client")?;
                front
                    .send_to(bytes, to)
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
        })
    };

    let client = run_client(binary, case, port, true);
    thread::sleep(UDP_DRAIN);
    stop.store(true, Ordering::Relaxed);
    for relay in [forward, backward] {
        relay
            .join()
            .map_err(|_| "a relay thread panicked".to_owned())??;
    }
    Ok((take(&sample)?, client?))
}

fn clone_pair(front: &UdpSocket, back: &UdpSocket) -> Result<(UdpSocket, UdpSocket), String> {
    Ok((
        front.try_clone().map_err(|error| error.to_string())?,
        back.try_clone().map_err(|error| error.to_string())?,
    ))
}

fn timed_out(error: &std::io::Error) -> bool {
    matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

fn append(sample: &Mutex<Vec<u8>>, direction: Direction, bytes: &[u8]) -> Result<(), String> {
    let mut file = sample.lock().map_err(|_| "poisoned".to_owned())?;
    sample::append(&mut file, direction, bytes)
}

fn take(sample: &Mutex<Vec<u8>>) -> Result<Vec<u8>, String> {
    Ok(std::mem::take(
        &mut *sample.lock().map_err(|_| "poisoned".to_owned())?,
    ))
}

/// The server process, killed when the recording fails part way.
struct KillOnDrop(Option<Child>);

impl KillOnDrop {
    /// Stops the server with SIGINT, so it flushes its report, and returns what it printed.
    fn interrupt(mut self) -> Result<Output, String> {
        let child = self.0.take().ok_or("the server is gone")?;
        let status = Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .map_err(|error| format!("kill: {error}"))?;
        if !status.success() {
            return Err("kill -INT failed".to_owned());
        }
        child
            .wait_with_output()
            .map_err(|error| format!("the server: {error}"))
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

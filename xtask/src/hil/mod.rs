//! `cargo xtask hil`: builds the S31 firmware, flashes it, and runs our client and our server on the board against
//! iperf 2.2.2 and our `std` driver on this machine (T-7). The installed `mcu-run` flashes the board and sends each
//! console line. The board's client is `mcu-net`'s `net iperf`, and its server is the firmware's `iperf -s`.
//!
//! Each run checks that the receiver lost no datagram and saw none out of order, that both ends count the same
//! bytes, and that a test in time mode lasted its length at the receiver. Our `std` driver runs in this process, in
//! the build profile of `xtask`.

mod reply;

use crate::iperf;
use embedded_iperf2::blocking::{TcpServer, UdpServer, run_client};
use embedded_iperf2::{Client, Config, Length, Report, Server, ServerConfig, Transport};
use reply::Counts;
use std::net::{IpAddr, Ipv4Addr, TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// The firmware's folder, its target and its binary.
const FIRMWARE: &str = "firmware/esp32-s31";
const TARGET: &str = "riscv32imafc-unknown-none-elf";
const BINARY: &str = "embedded-iperf2-esp32-s31";

/// The log line the firmware writes once its console runs.
const STARTED: &str = "console firmware";

/// How long the board may take to boot, and then to take a lease.
const BOOT: Duration = Duration::from_secs(60);
const LEASE: Duration = Duration::from_secs(30);

/// How long a console command may take beyond its test.
const SLACK: Duration = Duration::from_secs(20);

/// How long the host waits after it starts the board's server before its client starts. `mcu-run console` prints a
/// reply only once the command ends, so the host cannot see the server listen.
const LISTEN_WAIT: Duration = Duration::from_secs(2);

/// How long a stock server runs before the board's client starts.
const STOCK_START: Duration = Duration::from_millis(300);

/// The port the board's server listens on.
const BOARD_PORT: u16 = 5001;

/// The longest our `std` server waits for a test and lets one last (D-7, P-15).
const HOST_LIMIT: Duration = Duration::from_secs(60);

/// The length of our `std` ends' work buffers. A reverse TCP test sends bursts of up to 128 KiB.
const WORK: usize = 131_072;

/// The default burst of an iperf 2.2.2 server in a reverse TCP test (P-16).
const STOCK_BURST: u64 = 131_072;

/// How much shorter than its length a test in time mode may last at its receiver, in hundredths of a second.
const SHORT_BY: u64 = 25;

/// One test, as the options of `net iperf` and of a stock client give it.
struct Case {
    transport: Transport,
    reverse: bool,
    length: Length,
    /// The rate of a UDP test in bits per second. A TCP test sends as fast as it can.
    rate: u64,
    /// How many times the test runs.
    times: u32,
}

impl Case {
    fn args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if self.transport == Transport::Udp {
            args.push("-u".to_owned());
        }
        if self.reverse {
            args.push("-R".to_owned());
        }
        match self.length {
            Length::Time { us } => args.extend(["-t".to_owned(), (us / 1_000_000).to_string()]),
            Length::Bytes(count) => args.extend(["-n".to_owned(), count.to_string()]),
        }
        if self.transport == Transport::Udp {
            args.extend(["-b".to_owned(), self.rate.to_string()]);
        }
        args
    }

    fn name(&self) -> String {
        let transport = match self.transport {
            Transport::Tcp => "tcp",
            Transport::Udp => "udp",
        };
        let args = self.args();
        let rest = args.iter().filter(|arg| *arg != "-u");
        format!(
            "{transport} {}",
            rest.map(String::as_str).collect::<Vec<_>>().join(" ")
        )
    }

    fn config(&self, port: u16) -> Config {
        let base = match self.transport {
            Transport::Tcp => Config::tcp(),
            Transport::Udp => Config::udp(),
        };
        Config {
            reverse: self.reverse,
            length: self.length,
            rate: if self.transport == Transport::Udp {
                self.rate
            } else {
                0
            },
            port,
            ..base
        }
    }

    /// How long a console command of this test may take.
    fn timeout(&self) -> Duration {
        let seconds = match self.length {
            Length::Time { us } => us / 1_000_000,
            Length::Bytes(_) => 10,
        };
        Duration::from_secs(seconds).saturating_add(SLACK)
    }
}

/// T-5's modes at 10 Mbit/s for UDP, then the tests in which the devkit saw faults on 2026-10-18: UDP at 95 Mbit/s,
/// and a reverse TCP test that ended early once in six.
fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for transport in [Transport::Tcp, Transport::Udp] {
        for reverse in [false, true] {
            let bytes = match transport {
                Transport::Tcp => 20_000_000,
                Transport::Udp => 2_500_000,
            };
            for length in [Length::Time { us: 3_000_000 }, Length::Bytes(bytes)] {
                cases.push(Case {
                    transport,
                    reverse,
                    length,
                    rate: 10_000_000,
                    times: 1,
                });
            }
        }
    }
    for reverse in [false, true] {
        cases.push(Case {
            transport: Transport::Udp,
            reverse,
            length: Length::Time { us: 5_000_000 },
            rate: 95_000_000,
            times: 3,
        });
    }
    cases.push(Case {
        transport: Transport::Tcp,
        reverse: true,
        length: Length::Time { us: 5_000_000 },
        rate: 0,
        times: 10,
    });
    cases
}

/// Which end of the test the board runs.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Board {
    Client,
    Server,
}

/// What runs on this machine.
#[derive(Clone, Copy, Debug)]
enum Host {
    Stock,
    Ours,
}

/// Builds and flashes the firmware, then runs each case whose name holds `only`, or every case. Fails when a run
/// fails, after every run.
pub fn run(root: &Path, probe: &str, only: Option<&str>) -> Result<(), String> {
    let stock = std::env::var("MCU_IPERF2").map_err(|_| {
        "MCU_IPERF2 must name an iperf 2.2.2 binary, which `cargo xtask install-iperf` builds"
            .to_owned()
    })?;
    iperf::check_version(Path::new(&stock))?;
    let elf = build(&root.join(FIRMWARE))?;
    flash(probe, &elf)?;
    let board = lease(probe)?;
    let host = host_address(board)?;
    println!("==> the board is {board}, and this machine is {host}");
    let mut failed = 0_u32;
    for case in cases() {
        if only.is_some_and(|only| !case.name().contains(only)) {
            continue;
        }
        for end in [Board::Client, Board::Server] {
            for side in [Host::Stock, Host::Ours] {
                for _ in 0..case.times {
                    let label = format!("board {end:?} against {side:?}: {}", case.name());
                    match run_one(&stock, probe, board, host, &case, end, side) {
                        Ok(summary) => println!("ok   {label}: {summary}"),
                        Err(error) => {
                            failed = failed.saturating_add(1);
                            println!("FAIL {label}: {error}");
                        }
                    }
                }
            }
        }
    }
    if failed > 0 {
        return Err(format!("{failed} runs failed"));
    }
    println!("==> every run passed");
    Ok(())
}

/// Runs `case` once, and checks the board's report against this machine's.
fn run_one(
    stock: &str,
    probe: &str,
    board: Ipv4Addr,
    host: IpAddr,
    case: &Case,
    end: Board,
    side: Host,
) -> Result<String, String> {
    let (printed, ours) = match end {
        Board::Client => {
            let (port, server) = match side {
                Host::Stock => {
                    let port = free_port(case.transport)?;
                    let mut args = vec!["-s".to_owned(), "-p".to_owned(), port.to_string()];
                    if case.transport == Transport::Udp {
                        args.push("-u".to_owned());
                    }
                    (port, Waiting::Stock(StockServer::start(stock, &args)?))
                }
                Host::Ours => {
                    let (port, thread) = serve(case.transport)?;
                    (port, Waiting::Ours(thread))
                }
            };
            let line = format!("net iperf {host}:{port} {}", case.args().join(" "));
            let printed = send(probe, &line, case.timeout());
            let ours = server.counts();
            (printed?, ours?)
        }
        Board::Server => {
            let mut line = format!("iperf -s -p {BOARD_PORT}");
            if case.transport == Transport::Udp {
                line.push_str(" -u");
            }
            let (probe, limit) = (probe.to_owned(), case.timeout());
            let server = thread::spawn(move || send_stopped_after(&probe, &line, limit));
            thread::sleep(LISTEN_WAIT);
            let ours = match side {
                Host::Stock => stock_client(stock, board, case),
                Host::Ours => our_client(board, case),
            };
            let printed = server
                .join()
                .map_err(|_| "the console thread panicked".to_owned())?;
            (printed?, ours?)
        }
    };
    let theirs = reply::board(&printed, "result")?;
    check(case, end, side, &theirs, &ours)?;
    Ok(format!(
        "{} bytes, {}.{:02} s at the board",
        theirs.bytes,
        theirs.centiseconds / 100,
        theirs.centiseconds % 100
    ))
}

/// Checks the board's report against this machine's.
fn check(case: &Case, end: Board, side: Host, board: &Counts, host: &Counts) -> Result<(), String> {
    let board_receives = (end == Board::Client) == case.reverse;
    let (receiver, sender) = if board_receives {
        (board, host)
    } else {
        (host, board)
    };
    if let Some((lost, out_of_order)) = receiver.udp
        && (lost, out_of_order) != (0, 0)
    {
        return Err(format!(
            "the receiver lost {lost} and saw {out_of_order} out of order: board {board:?}, host {host:?}"
        ));
    }
    // An iperf 2.2.2 server in a reverse TCP test in byte mode sends whole bursts past the count, up to one default
    // burst more, and our client stops once the count has arrived.
    let stock_sends_bursts = case.transport == Transport::Tcp
        && case.reverse
        && end == Board::Client
        && matches!(side, Host::Stock);
    let counted = match case.length {
        Length::Bytes(count) if stock_sends_bursts => {
            count <= receiver.bytes
                && receiver.bytes <= sender.bytes
                && sender.bytes <= count.saturating_add(STOCK_BURST)
        }
        _ => receiver.bytes == sender.bytes,
    };
    if !counted {
        return Err(format!(
            "the receiver counted {} bytes and the sender {}",
            receiver.bytes, sender.bytes
        ));
    }
    if let Length::Time { us } = case.length {
        let wanted = us / 10_000;
        if receiver.centiseconds.saturating_add(SHORT_BY) < wanted {
            return Err(format!(
                "the receiver's test lasted {} hundredths of a second of {wanted}",
                receiver.centiseconds
            ));
        }
    }
    Ok(())
}

/// The host server of a board client's test. Its report comes once the test ends.
enum Waiting {
    Stock(StockServer),
    Ours(JoinHandle<Result<Counts, String>>),
}

impl Waiting {
    fn counts(self) -> Result<Counts, String> {
        match self {
            Self::Stock(server) => reply::stock(&server.interrupt()?),
            Self::Ours(thread) => thread
                .join()
                .map_err(|_| "our server's thread panicked".to_owned())?,
        }
    }
}

/// Starts our `std` server for one test on a free port, and returns the port and the thread that runs it.
fn serve(transport: Transport) -> Result<(u16, JoinHandle<Result<Counts, String>>), String> {
    let config = ServerConfig {
        transport,
        duration_us: Some(micros(HOST_LIMIT)),
        interval_us: None,
        ipv6: false,
    };
    let failed = |error: std::io::Error| error.to_string();
    match transport {
        Transport::Tcp => {
            let listener = TcpListener::bind("0.0.0.0:0").map_err(failed)?;
            let port = listener.local_addr().map_err(failed)?.port();
            let mut driver = TcpServer::new(listener);
            driver.set_start_timeout(Some(HOST_LIMIT)).map_err(failed)?;
            let thread = thread::spawn(move || {
                one_server_test(config, |server, on_report| {
                    driver.run_one(server, on_report)
                })
            });
            Ok((port, thread))
        }
        Transport::Udp => {
            let socket = UdpSocket::bind("0.0.0.0:0").map_err(failed)?;
            let port = socket.local_addr().map_err(failed)?.port();
            let mut driver = UdpServer::new(socket);
            driver.set_start_timeout(Some(HOST_LIMIT)).map_err(failed)?;
            let thread = thread::spawn(move || {
                one_server_test(config, |server, on_report| {
                    driver.run_one(server, on_report)
                })
            });
            Ok((port, thread))
        }
    }
}

/// Runs one test of a server with `config` through `run_one`, and returns its final report.
fn one_server_test(
    config: ServerConfig,
    mut run_one: impl FnMut(
        &mut Server<'_>,
        &mut dyn FnMut(&Report),
    ) -> std::io::Result<Result<(), embedded_iperf2::TestError>>,
) -> Result<Counts, String> {
    let mut buffer = vec![0; WORK];
    let mut server = Server::new(config, &mut buffer).map_err(|error| format!("{error:?}"))?;
    let mut last = None;
    let outcome = run_one(&mut server, &mut |report: &Report| {
        if report.final_report {
            last = Some(*report);
        }
    });
    ended(outcome, last)
}

/// Runs our `std` client of `case` against the board's server.
fn our_client(board: Ipv4Addr, case: &Case) -> Result<Counts, String> {
    let mut buffer = vec![0; WORK];
    let mut client =
        Client::new(case.config(BOARD_PORT), &mut buffer).map_err(|error| format!("{error:?}"))?;
    let mut last = None;
    let outcome = run_client(&mut client, (board, BOARD_PORT).into(), |report| {
        if report.final_report {
            last = Some(*report);
        }
    });
    ended(outcome, last)
}

/// The counts of a final report from our driver, or why the test failed.
fn ended(
    outcome: std::io::Result<Result<(), embedded_iperf2::TestError>>,
    last: Option<Report>,
) -> Result<Counts, String> {
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(format!("our end: {error:?}")),
        Err(error) => return Err(format!("our end: {error}")),
    }
    let report = last.ok_or("our end gave no final report")?;
    Ok(Counts {
        centiseconds: report.end_us.saturating_sub(report.start_us) / 10_000,
        bytes: report.bytes,
        udp: report.udp.map(|udp| (udp.lost, udp.out_of_order)),
    })
}

/// Runs a stock client of `case` against the board's server, and returns its last report.
fn stock_client(stock: &str, board: Ipv4Addr, case: &Case) -> Result<Counts, String> {
    let output = Command::new(stock)
        .args(["-c", &board.to_string(), "-p", &BOARD_PORT.to_string()])
        .args(case.args())
        .args(["-y", "C"])
        .output()
        .map_err(|error| format!("{stock}: {error}"))?;
    reply::stock(&String::from_utf8_lossy(
        &[output.stdout, output.stderr].concat(),
    ))
}

/// A stock server, killed when it is dropped.
struct StockServer(Option<Child>);

impl StockServer {
    fn start(stock: &str, args: &[String]) -> Result<Self, String> {
        let child = Command::new(stock)
            .args(args)
            .args(["-y", "C"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("{stock}: {error}"))?;
        thread::sleep(STOCK_START);
        Ok(Self(Some(child)))
    }

    /// Stops the server with SIGINT, so it flushes its report, and returns what it printed.
    fn interrupt(mut self) -> Result<String, String> {
        let child = self.0.take().ok_or("the stock server is gone")?;
        let status = Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .map_err(|error| format!("kill: {error}"))?;
        if !status.success() {
            return Err("kill -INT failed".to_owned());
        }
        let output = child
            .wait_with_output()
            .map_err(|error| format!("the stock server: {error}"))?;
        Ok(String::from_utf8_lossy(&[output.stdout, output.stderr].concat()).into_owned())
    }
}

impl Drop for StockServer {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Builds the firmware and returns its ELF.
fn build(firmware: &Path) -> Result<PathBuf, String> {
    println!("==> build {}", firmware.display());
    let status = Command::new("cargo")
        .current_dir(firmware)
        .args(["build", "--release"])
        .status()
        .map_err(|error| format!("could not start cargo: {error}"))?;
    if !status.success() {
        return Err("the firmware did not build".to_owned());
    }
    Ok(firmware
        .join("target")
        .join(TARGET)
        .join("release")
        .join(BINARY))
}

/// Flashes `elf` and follows the board's log until the console runs.
fn flash(probe: &str, elf: &Path) -> Result<(), String> {
    println!("==> flash {}", elf.display());
    let status = Command::new("mcu-run")
        .args([
            "run", "--chip", "esp32s31", "--probe", probe, "--until", STARTED,
        ])
        .arg("--timeout")
        .arg(BOOT.as_secs().to_string())
        .arg(elf)
        .stdout(Stdio::null())
        .status()
        .map_err(|error| format!("could not start mcu-run: {error}"))?;
    if !status.success() {
        return Err(format!("mcu-run run failed with {status}"));
    }
    Ok(())
}

/// Polls `net config` until the board holds a lease, and returns its address.
fn lease(probe: &str) -> Result<Ipv4Addr, String> {
    let start = Instant::now();
    loop {
        let printed = send(probe, "net config", Duration::from_secs(5))?;
        let field = |key: &str| {
            printed
                .lines()
                .find_map(|line| line.strip_prefix(key)?.strip_prefix(": "))
        };
        if field("link") == Some("up")
            && let Some(address) = field("address")
            && let Some(Ok(address)) = address.split('/').next().map(str::parse)
        {
            return Ok(address);
        }
        if start.elapsed() > LEASE {
            return Err(format!("no lease within {} s", LEASE.as_secs()));
        }
        thread::sleep(Duration::from_millis(500));
    }
}

/// This machine's address on the route to `board`, found without sending a packet.
fn host_address(board: Ipv4Addr) -> Result<IpAddr, String> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|error| error.to_string())?;
    socket
        .connect((board, 9))
        .map_err(|error| format!("no route to {board}: {error}"))?;
    Ok(socket.local_addr().map_err(|error| error.to_string())?.ip())
}

fn free_port(transport: Transport) -> Result<u16, String> {
    let port = match transport {
        Transport::Tcp => TcpListener::bind("0.0.0.0:0").and_then(|socket| socket.local_addr()),
        Transport::Udp => UdpSocket::bind("0.0.0.0:0").and_then(|socket| socket.local_addr()),
    };
    port.map(|address| address.port())
        .map_err(|error| error.to_string())
}

/// Sends one console line and returns what `mcu-run` printed, body lines then `ok`. An `err` result fails.
fn send(probe: &str, line: &str, timeout: Duration) -> Result<String, String> {
    run_mcu_run(
        line,
        Command::new("mcu-run")
            .args(["console", "--probe", probe, "--send", line, "--timeout"])
            .arg(timeout.as_secs().to_string()),
    )
}

/// Sends the line of a command that runs until stopped, and stops it with Ctrl-C after `limit`, so the board never
/// runs it past this machine's side of the test. `mcu-run` sends a second Ctrl-C at twice `limit`.
fn send_stopped_after(probe: &str, line: &str, limit: Duration) -> Result<String, String> {
    let timeout = limit.saturating_mul(2).saturating_add(SLACK);
    run_mcu_run(
        line,
        Command::new("mcu-run")
            .args(["console", "--probe", probe, "--send", line, "--timeout"])
            .arg(timeout.as_secs().to_string())
            .arg("--interrupt-after")
            .arg(limit.as_secs().to_string()),
    )
}

/// Runs `mcu-run` for `line` and returns what it printed, or why it failed.
fn run_mcu_run(line: &str, command: &mut Command) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|error| format!("could not start mcu-run: {error}"))?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        Ok(text)
    } else {
        Err(format!(
            "{line}: {}{}",
            text.trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

//! `cargo xtask fuzz [--seconds <n>]`: seeds the corpus with each record of each sample, then runs the fuzz target
//! on nightly. A crash input lands under `fuzz/artifacts/parse/`.

use crate::record::sample;
use std::fs;
use std::path::Path;
use std::process::Command;

/// The target in the `fuzz` crate.
const TARGET: &str = "parse";

/// The largest input the fuzzer builds. The longest message is the 128-byte server report. A UDP prefix can come
/// before a header.
const MAX_LEN: usize = 512;

/// Copies every sample message into the corpus, then runs the target on nightly for `seconds`.
pub fn run(root: &Path, seconds: u64) -> Result<(), String> {
    crate::shared::require_tool(root, "fuzz", "cargo install cargo-fuzz")?;
    seed_corpus(root)?;
    println!("==> fuzz {TARGET}");
    let status = Command::new("cargo")
        .current_dir(root)
        .args([
            &format!("+{}", crate::shared::toolchain::NIGHTLY),
            "fuzz",
            "run",
            TARGET,
            "--",
            &format!("-max_total_time={seconds}"),
            &format!("-max_len={MAX_LEN}"),
        ])
        .status()
        .map_err(|error| format!("could not start cargo fuzz: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "cargo fuzz found a crash or failed. A crash input is under fuzz/artifacts/{TARGET}/."
        ))
    }
}

/// Writes the first `MAX_LEN` bytes of each record of each sample to the corpus, as `<version>-<case>-<index>`. A
/// TCP record is one read, so it does not always start at a message. The fuzzer finds the other messages.
fn seed_corpus(root: &Path) -> Result<(), String> {
    let corpus = root.join("fuzz/corpus").join(TARGET);
    fs::create_dir_all(&corpus).map_err(|error| format!("{}: {error}", corpus.display()))?;
    let samples = root.join("embedded-iperf2/tests/samples");
    let versions =
        fs::read_dir(&samples).map_err(|error| format!("{}: {error}", samples.display()))?;
    let mut copied = 0usize;
    for version in versions.flatten() {
        let version_name = version.file_name().to_string_lossy().into_owned();
        let files = fs::read_dir(version.path())
            .map_err(|error| format!("{}: {error}", version.path().display()))?;
        for file in files.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|extension| extension != "bin") {
                continue;
            }
            let case = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            let records =
                sample::records(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
            for (index, record) in records.iter().enumerate() {
                let destination = corpus.join(format!("{version_name}-{case}-{index}"));
                let start = record.get(..MAX_LEN).unwrap_or(record);
                fs::write(&destination, start)
                    .map_err(|error| format!("{}: {error}", destination.display()))?;
                copied = copied.saturating_add(1);
            }
        }
    }
    println!("seeded the {TARGET} corpus with {copied} messages");
    Ok(())
}

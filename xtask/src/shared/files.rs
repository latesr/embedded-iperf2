// A copy from embedded-devkit/xtask/src/shared. Edit it there, then run `cargo xtask sync` in the devkit.

//! Finds the repo's source files for the checks that read them.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The rule files at the repo root that are copies from the devkit: `CONTRIBUTING.md`, the files its Where to look
/// table names, and `CONVENTIONS.md`.
pub const RULE_FILES: [&str; 8] = [
    "CONTRIBUTING.md",
    "CONVENTIONS.md",
    "CODE.md",
    "TESTING.md",
    "WRITING.md",
    "DOCS.md",
    "CHECKS.md",
    "EMBEDDED.md",
];

/// Every file under `root` with the given extension, keyed by its path relative to `root`. Skips `target`,
/// hidden directories, and the `mutants.out` directories, which hold build output and copies of the sources.
pub fn read_files(root: &Path, extension: &str) -> BTreeMap<PathBuf, String> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "target" && !name.starts_with('.') && !name.starts_with("mutants.out") {
                    pending.push(path);
                }
            } else if path.extension().is_some_and(|found| found == extension)
                && let (Ok(relative), Ok(text)) =
                    (path.strip_prefix(root), fs::read_to_string(&path))
            {
                files.insert(relative.to_path_buf(), text);
            }
        }
    }
    files
}

#[cfg(test)]
pub(crate) mod testing {
    use std::fs;
    use std::path::PathBuf;

    /// A throwaway repo under the temp directory, for tests of the checks that read files. It is removed on
    /// drop.
    pub(crate) struct TempRepo(pub(crate) PathBuf);

    impl TempRepo {
        pub(crate) fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("xtask-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join("docs")).unwrap();
            Self(dir)
        }

        pub(crate) fn write(&self, path: &str, content: &str) {
            fs::write(self.0.join(path), content).unwrap();
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

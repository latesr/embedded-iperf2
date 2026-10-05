// A copy from embedded-devkit/xtask/src/shared. Edit it there, then run `cargo xtask sync` in the devkit.

//! Writes the git hooks that run the checks.

use std::fs;
use std::path::Path;

const PRE_COMMIT: &str =
    "#!/bin/sh\ncargo xtask check && cargo xtask mutants --staged && exec cargo xtask prose\n";
const COMMIT_MSG: &str =
    "#!/bin/sh\ncargo xtask prose --message \"$1\" && exec cargo xtask links --message \"$1\"\n";

/// Writes `.git/hooks/pre-commit`, which runs `cargo xtask check`, then cargo-mutants over the staged lines, then
/// the prose check again so that its warnings are the last lines printed. Also writes `.git/hooks/commit-msg`,
/// which runs the prose check and the requirement ID check over the message.
pub fn install(root: &Path) -> Result<(), String> {
    let hooks = root.join(".git").join("hooks");
    if !hooks.is_dir() {
        return Err(format!(
            "{} does not exist. Is this a git repository?",
            hooks.display()
        ));
    }
    for (name, script) in [("pre-commit", PRE_COMMIT), ("commit-msg", COMMIT_MSG)] {
        let path = hooks.join(name);
        fs::write(&path, script).map_err(|error| format!("{}: {error}", path.display()))?;
        make_executable(&path)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

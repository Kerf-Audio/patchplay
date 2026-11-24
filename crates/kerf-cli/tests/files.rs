//! Where `kerf` will write: only to the extension the output is meant to have, and `new` never
//! through a path that already exists, even a dangling symlink. Offline: no audio device.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PATCH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../presets/first-cut.kerf");

fn kerf(args: &[&str], out: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kerf"))
        .args(args)
        .arg("-o")
        .arg(out)
        .output()
        .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kerf-cli-files-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn outputs_must_end_in_the_extension_they_are_for() {
    let dir = scratch("extension");
    let cases: [(&[&str], &str, &str); 5] = [
        (&["new"], "profile.sh", ".kerf"),
        (&["apply", PATCH, "-c", PATCH], "profile.sh", ".kerf"),
        (&["evolve", PATCH], "profile.sh", ".kerf"),
        (&["breed", PATCH, PATCH], "profile.sh", ".kerf"),
        (&["render", PATCH, "--seconds", "0.1"], "out.mp3", ".wav"),
    ];
    for (args, name, extension) in cases {
        let out = dir.join(name);
        let run = kerf(args, &out);
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(!run.status.success(), "{args:?} wrote {name}");
        assert!(
            stderr.contains(&format!("must end in {extension}")),
            "{stderr}"
        );
        assert!(!out.exists(), "{args:?} left {name} behind");
    }

    // Any case is fine, and the extension is all that is checked.
    let out = dir.join("LOUD.KERF");
    let run = kerf(&["new"], &out);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(out.exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn new_refuses_an_existing_path_even_a_dangling_symlink() {
    let dir = scratch("symlink");
    let taken = dir.join("taken.kerf");
    std::fs::write(&taken, "keep me").unwrap();
    let run = kerf(&["new"], &taken);
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("already exists"));
    assert_eq!(std::fs::read_to_string(&taken).unwrap(), "keep me");

    #[cfg(unix)]
    {
        let elsewhere = dir.join("elsewhere.kerf");
        let link = dir.join("link.kerf");
        std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
        let run = kerf(&["new"], &link);
        assert!(!run.status.success());
        assert!(String::from_utf8_lossy(&run.stderr).contains("already exists"));
        assert!(!elsewhere.exists(), "new wrote through a dangling symlink");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

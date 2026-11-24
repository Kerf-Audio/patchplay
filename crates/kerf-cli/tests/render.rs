//! `kerf render` refuses sample rates it cannot render at, instead of writing a broken file or
//! allocating gigabytes. Offline only: nothing here opens the audio device.

use std::path::PathBuf;
use std::process::{Command, Output};

fn render(rate: &str, out: &PathBuf) -> Output {
    let patch = concat!(env!("CARGO_MANIFEST_DIR"), "/../../presets/first-cut.kerf");
    Command::new(env!("CARGO_BIN_EXE_kerf"))
        .args(["render", patch, "--seconds", "0.1", "--rate", rate])
        .arg("-o")
        .arg(out)
        .output()
        .unwrap()
}

#[test]
fn render_rejects_rates_outside_the_supported_range() {
    let out = std::env::temp_dir().join(format!("kerf-cli-render-{}.wav", std::process::id()));
    for rate in ["0", "7999", "192001", "4000000000"] {
        let run = render(rate, &out);
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(!run.status.success(), "rate {rate} was accepted");
        assert!(stderr.contains("--rate"), "rate {rate}: {stderr}");
        assert!(!out.exists(), "rate {rate} left a file behind");
    }

    let run = render("8000", &out);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let bytes = std::fs::read(&out).unwrap();
    std::fs::remove_file(&out).unwrap();
    assert_eq!(&bytes[24..28], &8_000u32.to_le_bytes(), "the WAV header carries the rate");
}

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const TINY_GIF: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xff, 0xff, 0xff, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44,
    0x01, 0x00, 0x3b,
];

#[test]
fn cli_embed_inspect_extract_and_strip_round_trip() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_dir =
        std::env::temp_dir().join(format!("soundgif-cli-{}-{unique}", std::process::id()));
    fs::create_dir(&temp_dir).unwrap();

    let gif = temp_dir.join("input.gif");
    let audio = temp_dir.join("tone.opus");
    let encoded = temp_dir.join("encoded.gif");
    let extracted = temp_dir.join("extracted.opus");
    let stripped = temp_dir.join("stripped.gif");
    let audio_bytes: Vec<u8> = (0..2048).map(|value| (value % 239) as u8).collect();
    fs::write(&gif, TINY_GIF).unwrap();
    fs::write(&audio, &audio_bytes).unwrap();

    run(&[
        "embed",
        path(&gif),
        path(&audio),
        "-o",
        path(&encoded),
        "--start-ms",
        "75",
    ]);
    let inspect = run(&["inspect", path(&encoded)]);
    assert!(inspect.contains("MIME type:        audio/opus"));
    assert!(inspect.contains("Start offset:     75 ms"));
    assert!(inspect.contains("Integrity:        OK"));

    run(&["extract", path(&encoded), "-o", path(&extracted)]);
    assert_eq!(fs::read(&extracted).unwrap(), audio_bytes);

    run(&["strip", path(&encoded), "-o", path(&stripped)]);
    assert_eq!(fs::read(&stripped).unwrap(), TINY_GIF);

    fs::remove_dir_all(temp_dir).unwrap();
}

fn run(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_soundgif"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}

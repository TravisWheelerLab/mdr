use anyhow::Result;
use assert_cmd::Command;
use predicates::prelude::*;
use pretty_assertions::assert_eq;
use std::{fs, path::Path};

const PRG: &str = "mdr-meta";
const INPUT_ROOT: &str = "../libmdrepo/tests/inputs/metadata";

struct RunArgs<'a> {
    args: &'a [String],
    stdout: Option<&'a str>,
    stderr: Option<&'a str>,
    exit_code: Option<i32>,
}

// --------------------------------------------------
#[test]
fn dies_no_args() -> Result<()> {
    Command::cargo_bin(PRG)?
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
    Ok(())
}

// --------------------------------------------------
fn input_path(filename: &str) -> String {
    let input = Path::new(INPUT_ROOT);
    input.join(filename).to_string_lossy().to_string()
}

// --------------------------------------------------
fn run(args: RunArgs) -> Result<()> {
    let mut cmd = Command::cargo_bin(PRG)?;
    if !args.args.is_empty() {
        cmd.args(args.args);
    }
    dbg!(&cmd);
    let output = cmd.output()?;

    if let Some(expected_file) = args.stdout {
        let expected = fs::read_to_string(&input_path(expected_file))?;
        let stdout = String::from_utf8(output.stdout).expect("invalid UTF-8");
        assert_eq!(stdout.trim(), expected.trim());
    }

    if let Some(expected_file) = args.stderr {
        let expected = fs::read_to_string(&input_path(expected_file))?;
        let stderr = String::from_utf8(output.stderr).expect("invalid UTF-8");
        assert_eq!(stderr.trim(), expected.trim());
    }

    if let Some(expected) = args.exit_code {
        assert_eq!(output.status.code(), Some(expected));
    }

    Ok(())
}

// --------------------------------------------------
#[test]
fn check_ok1() -> Result<()> {
    run(RunArgs {
        args: &["check".to_string(), input_path("ok1.toml")],
        stdout: Some("empty.txt"),
        stderr: None,
        exit_code: Some(0),
    })
}

// --------------------------------------------------
#[test]
fn check_bad1() -> Result<()> {
    run(RunArgs {
        args: &["check".to_string(), input_path("bad1.toml")],
        stdout: Some("expected.bad1.txt"),
        stderr: None,
        exit_code: Some(1),
    })
}

// --------------------------------------------------
// One bad file among several must still fail the run, or a batch check
// reports success while hiding the file that needs fixing.
#[test]
fn check_mixed_exits_1() -> Result<()> {
    run(RunArgs {
        args: &[
            "check".to_string(),
            input_path("ok1.toml"),
            input_path("bad1.toml"),
        ],
        stdout: None,
        stderr: None,
        exit_code: Some(1),
    })
}

// --------------------------------------------------
// Unparseable input is invalid metadata (exit 1), not a tool failure.
// bad2.toml is a v1-era file, so deny_unknown_fields rejects it outright.
#[test]
fn check_unparseable_exits_1() -> Result<()> {
    Command::cargo_bin(PRG)?
        .args(["check", &input_path("bad2.toml")])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("Failed to parse input"));

    Ok(())
}

// --------------------------------------------------
// A file that cannot be read is also reported per-file, so the remaining
// filenames are still checked.
#[test]
fn check_missing_file_exits_1() -> Result<()> {
    run(RunArgs {
        args: &["check".to_string(), input_path("no/such/file.toml")],
        stdout: None,
        stderr: None,
        exit_code: Some(1),
    })
}

// --------------------------------------------------
// Checking no files at all must not report success.
#[test]
fn check_no_files_fails() -> Result<()> {
    Command::cargo_bin(PRG)?
        .arg("check")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));

    Ok(())
}

// --------------------------------------------------
// Exit 2 is reserved for "the program could not do its job," so it must
// not be produced by ordinary invalid metadata.
#[test]
fn check_never_exits_2() -> Result<()> {
    let output = Command::cargo_bin(PRG)?
        .args(["check", &input_path("bad1.toml")])
        .output()?;

    assert_ne!(output.status.code(), Some(2));

    Ok(())
}

// --------------------------------------------------
#[test]
fn example_toml() -> Result<()> {
    run(RunArgs {
        args: &["eg".to_string()],
        stdout: Some("expected.example.toml"),
        stderr: None,
        exit_code: Some(0),
    })
}

// --------------------------------------------------
#[test]
fn example_json() -> Result<()> {
    run(RunArgs {
        args: &["eg".to_string(), "-f".to_string(), "json".to_string()],
        stdout: Some("expected.example.json"),
        stderr: None,
        exit_code: Some(0),
    })
}

// --------------------------------------------------
// tests/inputs/gen/ is a fixture *directory* that `gen` scans, so its
// expected output can't live inside it -- gen would pick the expected-output
// file itself up as an additional file. It lives one level up instead, in
// tests/inputs/ alongside the metadata-file fixtures under INPUT_ROOT (which
// is a different, libmdrepo-owned directory).
const GEN_INPUT_ROOT: &str = "tests/inputs/gen";
const GEN_EXPECTED: &str = "tests/inputs/expected.gen.toml";

#[test]
fn gen_gromacs() -> Result<()> {
    let expected = fs::read_to_string(GEN_EXPECTED)?;

    let output = Command::cargo_bin(PRG)?
        .args(["gen", "-s", "GROMACS", "-d", GEN_INPUT_ROOT])
        .output()?;

    let stdout = String::from_utf8(output.stdout).expect("invalid UTF-8");
    assert_eq!(stdout.trim(), expected.trim());
    assert_eq!(output.status.code(), Some(0));

    Ok(())
}

// --------------------------------------------------
// clap validates --software against VALID_SOFTWARE itself (see
// PossibleValuesParser in types.rs), so an unknown engine never reaches
// generate() at all -- this is caught before the directory is even read.
#[test]
fn gen_rejects_invalid_software() -> Result<()> {
    Command::cargo_bin(PRG)?
        .args(["gen", "-s", "NOTAREALENGINE", "-d", GEN_INPUT_ROOT])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("possible values"));

    Ok(())
}

// --------------------------------------------------
#[test]
fn to_json() -> Result<()> {
    run(RunArgs {
        args: &["to-json".to_string(), input_path("ok1.toml")],
        stdout: Some("ok1.json"),
        stderr: None,
        exit_code: Some(0),
    })
}

// --------------------------------------------------
#[test]
fn to_toml() -> Result<()> {
    run(RunArgs {
        args: &["to-toml".to_string(), input_path("ok1.json")],
        stdout: Some("ok1.toml"),
        stderr: None,
        exit_code: Some(0),
    })
}

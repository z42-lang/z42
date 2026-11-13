//! Preserve Cargo messages before an outer Cargo plugin consumes them.
use std::{
    env,
    error::Error,
    fs::File,
    io::{self, Read, Write},
    process::{self, Command, Stdio},
};

fn run() -> Result<i32, Box<dyn Error>> {
    let cargo = env::var_os("Z42_CARGO_EXECUTABLE").ok_or("missing real Cargo executable")?;
    let args: Vec<_> = env::args_os().skip(1).collect();
    let structured = args.iter().any(|arg| {
        arg == "--message-format" || arg.to_string_lossy().starts_with("--message-format=")
    });
    let mut command = Command::new(&cargo);
    command.args(&args).env("CARGO", &cargo);
    if !structured {
        return Ok(command.status()?.code().unwrap_or(1));
    }
    let path = env::var_os("Z42_CARGO_ARTIFACT_LOG").ok_or("missing Cargo artifact log")?;
    let mut record = File::create(path)?;
    let mut child = command
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut output = child.stdout.take().ok_or("missing Cargo stdout")?;
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let mut bytes = [0u8; 32768];
    loop {
        let count = output.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        record.write_all(&bytes[..count])?;
        stdout.write_all(&bytes[..count])?;
    }
    record.flush()?;
    stdout.flush()?;
    Ok(child.wait()?.code().unwrap_or(1))
}

fn main() {
    match run() {
        Ok(code) => process::exit(code),
        Err(error) => {
            eprintln!("Cargo artifact capture: {error}");
            process::exit(1);
        }
    }
}

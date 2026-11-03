//! Optional rust-analyzer availability check. It never starts an LSP session.

use std::sync::mpsc::{self, Receiver};

pub fn check_async() -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let message = match std::process::Command::new("rust-analyzer")
            .arg("--version")
            .output()
        {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_owned()
            }
            Ok(output) => format!("rust-analyzer exited with {}", output.status),
            Err(error) => format!("rust-analyzer unavailable: {error}"),
        };
        let _ = sender.send(message);
    });
    receiver
}

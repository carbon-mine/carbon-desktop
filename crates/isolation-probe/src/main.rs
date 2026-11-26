//! Fixed benign M1 experiment. The launcher does not expose this binary via IPC.
#[cfg(windows)]
mod windows;

#[cfg(windows)]
fn main() -> std::io::Result<()> {
    windows::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Run sh scripts/linux-fixture.sh on Linux.");
    std::process::exit(2);
}

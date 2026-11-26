// SPDX-License-Identifier: MPL-2.0
use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=FixtureSession.java");
    println!("cargo:rerun-if-env-changed=JAVA_HOME");
    println!("cargo:rerun-if-env-changed=PATH");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("x86_64")
    {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo's output directory"));
    let classes = [
        "FixtureSession.class",
        "FixtureSession$1.class",
        "FixtureSession$IoAction.class",
    ];
    for class in classes {
        match fs::remove_file(out.join(class)) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => panic!("Could not remove the previous generated Java fixture: {e}"),
        }
    }
    let javac = env::var_os("JAVA_HOME")
        .map(|path| {
            PathBuf::from(path)
                .join("bin")
                .join(if cfg!(windows) { "javac.exe" } else { "javac" })
        })
        .filter(|path| path.is_file())
        .unwrap_or_else(|| "javac".into());
    let compiled = Command::new(javac)
        .args([
            "--release",
            "17",
            "-encoding",
            "UTF-8",
            "-proc:none",
            "-classpath",
        ])
        .arg(&out)
        .arg("-d")
        .arg(&out)
        .arg("FixtureSession.java")
        .env_remove("JAVA_TOOL_OPTIONS")
        .env_remove("_JAVA_OPTIONS")
        .env_remove("JDK_JAVA_OPTIONS")
        .env_remove("JDK_JAVAC_OPTIONS")
        .status();
    match compiled {
        Ok(status) => assert!(status.success(), "Could not compile the owned Java fixture"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("cargo:warning=JDK 17+ compiler unavailable; protected Java test session disabled in this build.");
            for class in classes {
                fs::write(out.join(class), []).expect("disabled fixture marker");
            }
        }
        Err(e) => panic!("Could not start the JDK compiler: {e}"),
    }
}

//! Inspect JDK/JRE metadata without executing a local or downloaded program.
use carbon_profiles::JavaChoice;
use serde::Serialize;
use std::{
    collections::HashSet,
    env, fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Serialize, Debug)]
pub struct RuntimeReport {
    pub state: &'static str,
    pub version: Option<String>,
    pub source: Option<&'static str>,
}

impl RuntimeReport {
    fn missing(source: Option<&'static str>) -> Self {
        Self {
            state: "missing",
            version: None,
            source,
        }
    }
}

fn field<'a>(release: &'a str, key: &str) -> Option<&'a str> {
    release
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(name, _)| *name == key)
        .map(|(_, v)| v.trim().trim_matches('"'))
}

pub fn inspect_home(home: &Path, required: u16, source: &'static str) -> Option<RuntimeReport> {
    if !home.is_absolute() {
        return None;
    }
    let executable = if cfg!(windows) { "java.exe" } else { "java" };
    if !home.join("bin").join(executable).is_file() {
        return None;
    }
    let release = home.join("release");
    let meta = fs::metadata(&release).ok()?;
    if !meta.is_file() || meta.len() > 65536 {
        return None;
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(release).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    file.take(65537).read_to_string(&mut text).ok()?;
    if text.len() > 65536 {
        return None;
    }
    let version = field(&text, "JAVA_VERSION")?;
    if version.len() > 64
        || !version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".+-_".contains(&b))
    {
        return None;
    }
    let major: u16 = version
        .strip_prefix("1.")
        .unwrap_or(version)
        .split(['.', '-', '+', '_'])
        .next()?
        .parse()
        .ok()?;
    let arch_ok = match (env::consts::ARCH, field(&text, "OS_ARCH")) {
        ("x86_64", Some("amd64" | "x86_64")) | ("aarch64", Some("aarch64" | "arm64")) => true,
        (native, Some(declared)) => native == declared,
        _ => false,
    };
    let os_ok = field(&text, "OS_NAME").is_some_and(|os| os.eq_ignore_ascii_case(env::consts::OS));
    Some(RuntimeReport {
        state: if major == required && arch_ok && os_ok {
            "compatible"
        } else {
            "incompatible"
        },
        version: Some(version.to_owned()),
        source: Some(source),
    })
}

pub fn inspect(
    required: u16,
    choice: Option<&JavaChoice>,
    managed_home: Option<&Path>,
) -> RuntimeReport {
    if let Some(JavaChoice::Directory(home)) = choice {
        return inspect_home(home, required, "Selected Java folder")
            .unwrap_or_else(|| RuntimeReport::missing(Some("Selected Java folder")));
    }
    if let Some(home) = managed_home {
        if let Some(report) = inspect_home(home, required, "Carbon-managed Temurin") {
            return report;
        }
    }
    if matches!(choice, Some(JavaChoice::Managed)) {
        return RuntimeReport::missing(Some("Carbon-managed Temurin"));
    }
    let mut candidates: Vec<(PathBuf, &'static str)> = Vec::new();
    if let Some(home) = env::var_os("JAVA_HOME") {
        candidates.push((home.into(), "JAVA_HOME"));
    }
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    if let Some(path) = env::var_os("PATH") {
        for dir in env::split_paths(&path)
            .filter(|dir| dir.is_absolute())
            .take(256)
        {
            if let Ok(binary) = dir.join(exe).canonicalize() {
                if let Some(home) = binary.parent().and_then(Path::parent) {
                    candidates.push((home.to_owned(), "System path"));
                }
            }
        }
    }
    let mut seen = HashSet::new();
    let mut fallback = RuntimeReport::missing(None);
    for (home, source) in candidates {
        if seen.insert(home.clone()) {
            if let Some(report) = inspect_home(&home, required, source) {
                if report.state == "compatible" {
                    return report;
                }
                if fallback.version.is_none() {
                    fallback = report;
                }
            }
        }
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn passive_check_requires_matching_declared_os_major_and_architecture() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        fs::create_dir(home.join("bin")).unwrap();
        fs::write(
            home.join("bin")
                .join(if cfg!(windows) { "java.exe" } else { "java" }),
            b"inert fixture",
        )
        .unwrap();
        for (os, major, arch, expected) in [
            (env::consts::OS, "25", env::consts::ARCH, "compatible"),
            (env::consts::OS, "21", env::consts::ARCH, "incompatible"),
            ("unknown", "25", env::consts::ARCH, "incompatible"),
            (env::consts::OS, "25", "unknown", "incompatible"),
        ] {
            fs::write(
                home.join("release"),
                format!("JAVA_VERSION=\"{major}.0.1\"\nOS_ARCH=\"{arch}\"\nOS_NAME=\"{os}\"\n"),
            )
            .unwrap();
            assert_eq!(inspect_home(home, 25, "test").unwrap().state, expected);
        }
        fs::write(home.join("release"), vec![b'x'; 65537]).unwrap();
        assert!(inspect_home(home, 25, "test").is_none());
    }
}

// SPDX-License-Identifier: MPL-2.0

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    Zip,
    TarGz,
}

pub(crate) struct Artifact {
    pub(crate) release: &'static str,
    pub(crate) format: Format,
    pub(crate) url: &'static str,
    pub(crate) archive_bytes: u64,
    pub(crate) sha256: &'static str,
}

impl Artifact {
    pub(crate) fn compiler(&self) -> &'static str {
        match self.format {
            Format::Zip => "bin/javac.exe",
            Format::TarGz => "bin/javac",
        }
    }

    pub(crate) fn executable(&self) -> &'static str {
        match self.format {
            Format::Zip => "bin/java.exe",
            Format::TarGz => "bin/java",
        }
    }
}

pub(crate) fn artifact(major: u16) -> Option<&'static Artifact> {
    if !cfg!(target_arch = "x86_64") {
        return None;
    }
    match (major, std::env::consts::OS) {
        (17, "windows") => Some(&Artifact {
            release: "17.0.21.1+2",
            format: Format::Zip,
            url: "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.21.1%2B2/OpenJDK17U-jdk_x64_windows_hotspot_17.0.21.1_2.zip",
            archive_bytes: 191_150_540,
            sha256: "eafba0944f777abdc74d5aad16049c9b5a1275b6ce99e2939ba9f7f3e6f08c87",
        }),
        (17, "linux") => Some(&Artifact {
            release: "17.0.21.1+2",
            format: Format::TarGz,
            url: "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.21.1%2B2/OpenJDK17U-jdk_x64_linux_hotspot_17.0.21.1_2.tar.gz",
            archive_bytes: 193_630_735,
            sha256: "0b0ac5eab6db6abaec68c9913bcddb31321e5da411c45073d520d9c125e6f7b4",
        }),
        (21, "windows") => Some(&Artifact {
            release: "21.0.13.1+2",
            format: Format::Zip,
            url: "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.13.1%2B2/OpenJDK21U-jdk_x64_windows_hotspot_21.0.13.1_2.zip",
            archive_bytes: 205_447_663,
            sha256: "bbebea1469996b276683c9299c227b294f85c847039c82633218186a4ad4fbf2",
        }),
        (21, "linux") => Some(&Artifact {
            release: "21.0.13.1+2",
            format: Format::TarGz,
            url: "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.13.1%2B2/OpenJDK21U-jdk_x64_linux_hotspot_21.0.13.1_2.tar.gz",
            archive_bytes: 207_869_899,
            sha256: "bb8a93f9ae6e7a693b2b721e85dbeff26ae1a77447985a8d7afcdd9c7cf6222e",
        }),
        (25, "windows") => Some(&Artifact {
            release: "25.0.5.1+2",
            format: Format::Zip,
            url: "https://github.com/adoptium/temurin25-binaries/releases/download/jdk-25.0.5.1%2B2/OpenJDK25U-jdk_x64_windows_hotspot_25.0.5.1_2.zip",
            archive_bytes: 141_295_462,
            sha256: "849d380ab0a64d9098e7d3a2de28beb73841c3b255c8284f019ccd8748c5eb5b",
        }),
        (25, "linux") => Some(&Artifact {
            release: "25.0.5.1+2",
            format: Format::TarGz,
            url: "https://github.com/adoptium/temurin25-binaries/releases/download/jdk-25.0.5.1%2B2/OpenJDK25U-jdk_x64_linux_hotspot_25.0.5.1_2.tar.gz",
            archive_bytes: 141_362_230,
            sha256: "760f91b22c3efc9bd1b64ff950598231be82928cc807e30691a0e7a2995a62c0",
        }),
        _ => None,
    }
}

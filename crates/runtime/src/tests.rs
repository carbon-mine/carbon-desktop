// SPDX-License-Identifier: MPL-2.0
use super::*;
use artifact::Format;
use std::io::Cursor;
use zip::write::SimpleFileOptions;

const RELEASE: &str = "17.0.test+1";

fn required(exe: &'static str) -> [(&'static str, &'static [u8]); 6] {
    [
        (exe, b"inert java witness"),
        (
            if exe.ends_with(".exe") {
                "bin/javac.exe"
            } else {
                "bin/javac"
            },
            b"inert compiler witness",
        ),
        ("release", b"inert release witness"),
        ("lib/modules", b"inert modules witness"),
        ("legal/java.base/LICENSE", b"license witness"),
        ("legal/java.base/ASSEMBLY_EXCEPTION", b"notice witness"),
    ]
}

fn fixture_zip(additional: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().unix_permissions(0o644);
    for (name, content) in required("bin/java.exe")
        .into_iter()
        .chain(additional.iter().copied())
    {
        writer
            .start_file(format!("jdk-{RELEASE}/{name}"), opts)
            .unwrap();
        writer.write_all(content).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn fixture_tar(links: &[(&str, &str)]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut tar = tar::Builder::new(encoder);
    for (name, content) in required("bin/java") {
        let mut header = tar::Header::new_gnu();
        header.set_mode(if name.starts_with("bin/") {
            0o755
        } else {
            0o644
        });
        header.set_size(content.len() as u64);
        tar.append_data(&mut header, format!("jdk-{RELEASE}/{name}"), content)
            .unwrap();
    }
    for (name, target) in links {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_mode(0o777);
        header.set_size(0);
        tar.append_link(&mut header, format!("jdk-{RELEASE}/{name}"), target)
            .unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap()
}

fn spec(bytes: &[u8], format: Format) -> Artifact {
    Artifact {
        release: RELEASE,
        format,
        url: "https://github.com/adoptium",
        archive_bytes: bytes.len() as u64,
        sha256: Box::leak(hex::encode(Sha256::digest(bytes)).into_boxed_str()),
    }
}

fn install(root: &Path, bytes: &[u8], spec: &Artifact) -> RuntimeStatus {
    prepare_supported(root, 17, spec, &mut |_, _| true, |_, _| Ok(bytes.to_vec())).unwrap()
}

#[test]
fn verified_cache_reuses_without_network_and_repairs_modified_or_added_files() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("runtime");
    let bytes = fixture_zip(&[]);
    let spec = spec(&bytes, Format::Zip);
    let absent = inspect_supported(&root, 17, &spec).unwrap();
    assert!(matches!(absent, RuntimeStatus::Available { .. }));
    assert!(!root.exists());
    let first = install(&root, &bytes, &spec);
    let home = first.java_home().unwrap();
    let ready_status = prepare_supported(&root, 17, &spec, &mut |_, _| true, |_, _| {
        panic!("cache hit fetched input")
    })
    .unwrap();
    assert_eq!(ready_status.java_home().unwrap(), home);
    let json = serde_json::to_value(ready_status).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "status": "ready", "major": 17, "release": RELEASE, "downloadBytes": bytes.len()
        })
    );

    let java = home.join("bin/java.exe");
    fs::write(&java, vec![0; fs::metadata(&java).unwrap().len() as usize]).unwrap();
    assert!(matches!(
        inspect_supported(&root, 17, &spec).unwrap(),
        RuntimeStatus::RepairNeeded { .. }
    ));
    assert_eq!(install(&root, &bytes, &spec).java_home().unwrap(), home);
    assert_eq!(fs::read(&java).unwrap(), b"inert java witness");
    fs::write(home.join("lib/extra"), b"untracked").unwrap();
    assert!(matches!(
        inspect_supported(&root, 17, &spec).unwrap(),
        RuntimeStatus::RepairNeeded { .. }
    ));
    assert_eq!(install(&root, &bytes, &spec).java_home().unwrap(), home);
    assert!(!home.join("lib/extra").exists());
}

#[test]
fn rejects_unpinned_download_and_leaves_cancelled_cache_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let bytes = fixture_zip(&[]);
    let spec = spec(&bytes, Format::Zip);
    let mut changed = bytes.clone();
    changed[2] ^= 0xff;
    assert!(matches!(
        prepare_supported(tmp.path(), 17, &spec, &mut |_, _| true, |_, _| Ok(changed)),
        Err(RuntimeError::Integrity)
    ));

    let mut checks = 0;
    let result = prepare_supported(
        tmp.path(),
        17,
        &spec,
        &mut |_, _| {
            checks += 1;
            checks < 6
        },
        |_, _| Ok(bytes.clone()),
    );
    assert!(matches!(result, Err(RuntimeError::Cancelled)));
    assert!(!tmp.path().join(cache_name(17, &spec)).exists());
    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1); // persistent lock only
    let first = install(tmp.path(), &bytes, &spec);
    assert!(matches!(
        prepare_supported(tmp.path(), 17, &spec, &mut |_, _| false, |_, _| panic!(
            "cancelled"
        )),
        Err(RuntimeError::Cancelled)
    ));
    assert_eq!(
        inspect_supported(tmp.path(), 17, &spec)
            .unwrap()
            .java_home(),
        first.java_home()
    );
}

#[test]
fn rejects_extra_links_and_unsafe_zip_paths_without_publication() {
    let tmp = tempfile::tempdir().unwrap();
    for bad in &[
        &[("bin/../escape", b"bad".as_slice())][..],
        &[("BIN/Other", b"bad".as_slice())],
        &[("lib/NUL.txt", b"bad".as_slice())],
        &[("lib/modules/child", b"bad".as_slice())],
        &[("release.", b"bad".as_slice())],
    ] {
        let bytes = fixture_zip(bad);
        let spec = spec(&bytes, Format::Zip);
        assert!(matches!(
            prepare_supported(tmp.path(), 17, &spec, &mut |_, _| true, |_, _| Ok(
                bytes.clone()
            )),
            Err(RuntimeError::Archive(_))
        ));
        assert!(!tmp.path().join(cache_name(17, &spec)).exists());
    }
    let bytes = fixture_tar(&[("bin/alternate", "../legal/java.base/LICENSE")]);
    let spec = spec(&bytes, Format::TarGz);
    assert!(matches!(
        prepare_supported(tmp.path(), 17, &spec, &mut |_, _| true, |_, _| Ok(
            bytes.clone()
        )),
        Err(RuntimeError::Archive(_))
    ));
}

#[test]
fn linux_legal_link_is_recreated_as_a_private_file() {
    let tmp = tempfile::tempdir().unwrap();
    let bytes = fixture_tar(&[("legal/java.sql/LICENSE", "../java.base/LICENSE")]);
    let layout = spec(&bytes, Format::TarGz);
    let current = install(tmp.path(), &bytes, &layout);
    let home = current.java_home().unwrap();
    let target = home.join("legal/java.base/LICENSE");
    let copy = home.join("legal/java.sql/LICENSE");
    assert_eq!(fs::read(&copy).unwrap(), fs::read(&target).unwrap());
    assert!(!storage::is_link(&fs::symlink_metadata(&copy).unwrap()));
    fs::remove_file(&copy).unwrap();
    fs::hard_link(&target, &copy).unwrap();
    assert!(matches!(
        inspect_supported(tmp.path(), 17, &layout).unwrap(),
        RuntimeStatus::RepairNeeded { .. }
    ));
}

#[test]
fn stale_stage_is_discarded_and_a_competing_installer_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let bytes = fixture_zip(&[]);
    let spec = spec(&bytes, Format::Zip);
    let abandoned = tmp.path().join(".stage-java-17-AAAAAAAAAAAA");
    fs::create_dir(&abandoned).unwrap();
    fs::write(abandoned.join("source.archive"), b"incomplete").unwrap();
    let guard = storage::lock(tmp.path(), 17, true).unwrap();
    assert!(matches!(
        prepare_supported(tmp.path(), 17, &spec, &mut |_, _| true, |_, _| panic!(
            "locked"
        )),
        Err(RuntimeError::Busy)
    ));
    assert!(abandoned.exists());
    drop(guard);
    let result = install(tmp.path(), &bytes, &spec);
    assert!(result.java_home().unwrap().is_dir());
    assert!(!abandoned.exists());
}

#[test]
fn transport_accepts_only_approved_https_asset_hosts() {
    for url in [
        "http://github.com/adoptium",
        "https://github.com.example/adoptium",
        "https://www.github.com/adoptium",
        "https://github.com:8443/adoptium",
        "https://u:p@github.com/adoptium",
    ] {
        assert!(!approved_url(&reqwest::Url::parse(url).unwrap()));
    }
    assert!(approved_url(
        &reqwest::Url::parse("https://release-assets.githubusercontent.com/assets/123").unwrap()
    ));
}

#[test]
#[ignore = "downloads three pinned public runtimes into a disposable directory"]
fn live_native_temurin_preparation() {
    let tmp = tempfile::tempdir().unwrap();
    for major in [17, 21, 25] {
        let ready = prepare(tmp.path(), major).unwrap();
        let home = ready.java_home().unwrap();
        let artifact = artifact(major).unwrap();
        assert!(home.join(artifact.executable()).is_file());
        assert!(home.join(artifact.compiler()).is_file());
        let release = fs::read_to_string(home.join("release")).unwrap();
        assert!(release.contains(&format!("JAVA_VERSION=\"{major}.")));
        assert!(release.contains("Eclipse Adoptium"));
        assert_eq!(
            inspect(tmp.path(), major).unwrap().java_home().unwrap(),
            home
        );
        println!(
            "prepared and re-verified Java {major} {} without running it",
            artifact.release
        );
    }
}

// SPDX-License-Identifier: MPL-2.0
//! Installer-relative Home scenery package.
//!
//! The compiled catalog stays authoritative for identity, order, names and
//! attribution. A package beside the executable adds compressed motion and
//! matching posters for those same views, so higher-quality artwork ships as
//! installer resources instead of a rebuilt launcher. Any invalid package is
//! ignored as a whole and the bundled artwork remains the fallback.
use gpui::RenderImage;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, OnceLock},
};

pub(super) struct Movie {
    poster: PathBuf,
    motion: PathBuf,
    pub(super) fps: u8,
    pub(super) frames: u32,
}

impl Movie {
    pub(super) fn motion(&self) -> &Path {
        &self.motion
    }
    /// The packaged poster, decoded with the same JPEG path as bundled art.
    pub(super) fn poster(&self) -> Option<Arc<RenderImage>> {
        super::decode(&fs::read(&self.poster).ok()?)
    }
}

pub(super) struct Package {
    views: BTreeMap<String, Movie>,
}

impl Package {
    pub(super) fn view(&self, id: &str) -> Option<&Movie> {
        self.views.get(id)
    }

    fn load(root: &Path) -> Result<Self, String> {
        let manifest = root.join("manifest.json");
        let bytes = fs::read(&manifest)
            .map_err(|error| format!("Cannot read {}: {error}", manifest.display()))?;
        let manifest: Manifest = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid {}: {error}", manifest.display()))?;
        if manifest.version != 1 {
            return Err(format!(
                "Expected a version 1 scenery package, got {}",
                manifest.version
            ));
        }
        if manifest.views.is_empty() {
            return Err("A scenery package needs at least one view".into());
        }
        let mut views = BTreeMap::new();
        for view in manifest.views {
            if super::Landscape::from_id(&view.id).is_none() {
                return Err(format!("Unknown packaged scene {}", view.id));
            }
            if views.contains_key(&view.id) {
                return Err(format!("Duplicate packaged scene {}", view.id));
            }
            if !(1..=60).contains(&view.fps) {
                return Err(format!(
                    "Packaged scene {} needs 1-60 fps, got {}",
                    view.id, view.fps
                ));
            }
            let poster = local_file(root, &view.poster, &[".jpg", ".jpeg"])?;
            if !(1..=3600).contains(&view.frames) {
                return Err(format!(
                    "Packaged scene {} needs 1-3600 frames, got {}",
                    view.id, view.frames
                ));
            }
            let motion = local_file(root, &view.motion, &[".mp4"])?;
            views.insert(
                view.id,
                Movie {
                    poster,
                    motion,
                    fps: view.fps,
                    frames: view.frames,
                },
            );
        }
        Ok(Self { views })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u16,
    views: Vec<View>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct View {
    id: String,
    poster: String,
    motion: String,
    frames: u32,
    fps: u8,
}

fn local_file(root: &Path, name: &str, extensions: &[&str]) -> Result<PathBuf, String> {
    let path = Path::new(name);
    let mut components = path.components();
    let single =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
    if !single || name.starts_with('.') {
        return Err(format!("Expected a local scenery file name, got {name}"));
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    if !extension.is_some_and(|extension| {
        extensions
            .iter()
            .any(|accepted| accepted.trim_start_matches('.') == extension)
    }) {
        return Err(format!(
            "Expected one of {} for {name}",
            extensions.join(", ")
        ));
    }
    let full = root.join(path);
    if !fs::metadata(&full).is_ok_and(|meta| meta.is_file()) {
        return Err(format!("Missing packaged scenery file {name}"));
    }
    Ok(full)
}

/// Selects the package once per process: an explicit absolute
/// `--scenery-package` directory, otherwise `scenery/` beside the executable.
pub(super) fn discover() -> Option<&'static Package> {
    static PACKAGE: OnceLock<Option<Package>> = OnceLock::new();
    PACKAGE
        .get_or_init(|| {
            let root = root()?;
            match Package::load(&root) {
                Ok(package) => {
                    eprintln!(
                        "Packaged scenery: {} views from {}",
                        package.views.len(),
                        root.display()
                    );
                    Some(package)
                }
                Err(error) => {
                    eprintln!("Ignoring packaged scenery at {}: {error}", root.display());
                    None
                }
            }
        })
        .as_ref()
}

fn root() -> Option<PathBuf> {
    let args: Vec<_> = std::env::args_os().collect();
    if let Some(index) = args.iter().position(|arg| arg == "--scenery-package") {
        let path = PathBuf::from(args.get(index + 1)?);
        return (path.is_absolute() && path.is_dir()).then_some(path);
    }
    let executable = std::env::current_exe().ok()?;
    let directory = executable.parent()?.join("scenery");
    directory.is_dir().then_some(directory)
}

#[cfg(all(test, windows))]
impl Movie {
    pub(super) fn test(poster: PathBuf, motion: PathBuf, frames: u32) -> Self {
        Self {
            poster,
            motion,
            fps: 30,
            frames,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn package(files: &[(&str, &str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let mut views = Vec::new();
        for (id, poster, motion) in files {
            fs::write(root.path().join(poster), b"poster").unwrap();
            fs::write(root.path().join(motion), b"motion").unwrap();
            views.push(format!(
                r#"{{"id":"{id}","poster":"{poster}","motion":"{motion}","frames":240,"fps":30}}"#
            ));
        }
        fs::write(
            root.path().join("manifest.json"),
            format!(r#"{{"version":1,"views":[{}]}}"#, views.join(",")),
        )
        .unwrap();
        root
    }

    #[test]
    fn a_valid_package_exposes_its_views() {
        let root = package(&[("riverside", "riverside.jpg", "riverside.mp4")]);
        let package = Package::load(root.path()).unwrap();
        let movie = package.view("riverside").unwrap();
        assert_eq!((movie.fps, movie.frames), (30, 240));
        assert!(movie.motion().exists());
    }

    #[test]
    fn unsafe_or_incomplete_packages_are_rejected() {
        let root = package(&[("riverside", "riverside.jpg", "riverside.mp4")]);
        let manifest = fs::read_to_string(root.path().join("manifest.json")).unwrap();
        let cases = [
            manifest.replace("riverside.jpg", "../riverside.jpg"),
            manifest.replace("riverside.jpg", "C:/riverside.jpg"),
            manifest.replace("riverside.mp4", "riverside.mkv"),
            manifest.replace("\"frames\":240", "\"frames\":0"),
            manifest.replace("\"fps\":30", "\"fps\":61"),
            manifest.replace("riverside", "unknown-scene"),
            manifest.replace("\"motion\":\"riverside.mp4\",", ""),
        ];
        for case in cases {
            fs::write(root.path().join("manifest.json"), case).unwrap();
            assert!(Package::load(root.path()).is_err());
        }
        fs::remove_file(root.path().join("riverside.mp4")).unwrap();
        fs::write(root.path().join("manifest.json"), manifest).unwrap();
        assert!(Package::load(root.path()).is_err());
    }

    #[test]
    fn duplicate_scenes_are_rejected() {
        let root = package(&[
            ("riverside", "riverside.jpg", "riverside.mp4"),
            ("riverside", "riverside-2.jpg", "riverside-2.mp4"),
        ]);
        assert!(Package::load(root.path()).is_err());
    }
}

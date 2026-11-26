// SPDX-License-Identifier: MPL-2.0
use super::PlanError;
use carbon_profiles::VanillaVersion;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CorpusEntry {
    pub version: VanillaVersion,
    sha256: String,
    pub java_major: u16,
    #[cfg(test)]
    pub sha1: String,
    #[cfg(test)]
    pub url: String,
    #[cfg(test)]
    pub asset_index: String,
    #[cfg(test)]
    pub asset_index_sha1: String,
    #[cfg(test)]
    pub windows_classpath: usize,
    #[cfg(test)]
    pub linux_classpath: usize,
}

static CORPUS: LazyLock<Result<Vec<CorpusEntry>, serde_json::Error>> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../fixtures/launch-corpus/vanilla.json"
    ))
});

pub(super) fn entries() -> Result<&'static [CorpusEntry], PlanError> {
    CORPUS.as_deref().map_err(|_| PlanError::UnreviewedManifest)
}

pub(super) fn reviewed(
    version: VanillaVersion,
    raw: &[u8],
) -> Result<&'static CorpusEntry, PlanError> {
    let entry = entries()?
        .iter()
        .find(|entry| entry.version == version)
        .ok_or(PlanError::UnsupportedVersion)?;
    if hex::encode(Sha256::digest(raw)) != entry.sha256 {
        return Err(PlanError::UnreviewedManifest);
    }
    Ok(entry)
}

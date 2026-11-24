use super::LoadedArtifact;
use crate::zpkg::reader::read_zpkg_meta;

#[derive(Clone)]
pub(super) struct PackageIdentity {
    pub(super) file: String,
    pub(super) digest: String,
    pub(super) dependencies: Vec<String>,
}

impl PackageIdentity {
    pub(super) fn from_artifact(artifact: &LoadedArtifact) -> Option<Self> {
        if !artifact.import_namespaces.is_empty() {
            return None;
        }
        Some(Self {
            file: format!("{}.zpkg", artifact.package_name.as_ref()?),
            digest: artifact.content_id.clone()?,
            dependencies: artifact.dependencies.iter().map(|dep| dep.file.clone()).collect(),
        })
    }

    pub(super) fn from_bytes(file: &str, bytes: &[u8]) -> Option<Self> {
        let meta = read_zpkg_meta(bytes).ok()?;
        // Indexed packages' files are verified at load time. The manifest
        // alone does not prove their current contents on a lazy edge.
        if !meta.is_packed || file != format!("{}.zpkg", meta.name) {
            return None;
        }
        Some(Self {
            file: file.to_owned(),
            digest: super::content_id(bytes),
            dependencies: meta.dependencies.into_iter().map(|dep| dep.file).collect(),
        })
    }
}

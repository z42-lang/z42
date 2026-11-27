use std::collections::BTreeMap;
use std::path::Path;

use super::LazyLoader;
use crate::loader::package_identity::PackageIdentity;

impl LazyLoader {
    /// Fingerprint the entry package and its executable dependency closure.
    /// Registered programs use the bytes that were decoded. For a lazy edge
    /// use the loader's candidate or its normal filename search order.
    pub(in crate::loader) fn program_content_identity(&self) -> Option<String> {
        let root = self.entry_identity.as_ref()?;
        let mut packages = BTreeMap::from([(root.file.clone(), root.digest.clone())]);
        let mut pending = root.dependencies.clone();
        pending.extend(self.package_identities.keys().cloned());
        while let Some(file) = pending.pop() {
            if packages.contains_key(&file) {
                continue;
            }
            let identity = self.dependency_identity(&file)?;
            packages.insert(file, identity.digest);
            pending.extend(identity.dependencies);
        }
        let mut hash = blake3::Hasher::new();
        hash.update(b"z42-program-content-v1\0");
        hash.update(&(root.file.len() as u64).to_le_bytes());
        hash.update(root.file.as_bytes());
        for (file, digest) in packages {
            hash.update(&(file.len() as u64).to_le_bytes());
            hash.update(file.as_bytes());
            hash.update(digest.as_bytes());
        }
        Some(hash.finalize().to_hex().to_string())
    }

    fn dependency_identity(&self, file: &str) -> Option<PackageIdentity> {
        if let Some(identity) = self.package_identities.get(file) {
            return Some(identity.clone());
        }
        if self.loaded_zpkgs.contains(file)
            || !file.ends_with(".zpkg")
            || Path::new(file).file_name()?.to_str()? != file
        {
            return None;
        }
        let path = match self.declared_zpkgs.get(file) {
            Some(candidate) => candidate.file_path.clone(),
            None => self.search_dirs.iter().map(|dir| dir.join(file)).find(|candidate| {
                candidate.to_str().is_some_and(|p| self.vfs.exists(p) && !self.vfs.is_dir(p))
            })?,
        };
        let bytes = self.vfs.read(path.to_str()?).ok()?;
        PackageIdentity::from_bytes(file, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(file: &str, content: &[u8], dependencies: &[&str]) -> PackageIdentity {
        PackageIdentity {
            file: file.into(),
            digest: crate::loader::content_id(content),
            dependencies: dependencies.iter().copied().map(str::to_owned).collect(),
        }
    }

    #[test]
    fn identity_includes_transitive_package_content() {
        let mut loader = LazyLoader::new(vec![], 0, vec![], vec![]);
        loader.entry_identity = Some(package("driver.zpkg", b"driver", &["pipeline.zpkg"]));
        loader.package_identities.insert(
            "pipeline.zpkg".into(),
            package("pipeline.zpkg", b"pipeline", &["emission.zpkg"]),
        );
        assert!(loader.program_content_identity().is_none());

        loader.package_identities.insert(
            "emission.zpkg".into(),
            package("emission.zpkg", b"old emission", &["pipeline.zpkg"]),
        );
        let before = loader.program_content_identity().unwrap();
        assert_eq!(loader.program_content_identity().as_deref(), Some(before.as_str()));

        loader.package_identities.insert(
            "emission.zpkg".into(),
            package("emission.zpkg", b"new emission", &["pipeline.zpkg"]),
        );
        assert_ne!(loader.program_content_identity().as_deref(), Some(before.as_str()));
    }
}

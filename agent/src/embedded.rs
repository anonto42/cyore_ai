// The int8 model compiled into the binary (Stage 4). build.rs puts either the real files or
// empty placeholders into OUT_DIR, so this always compiles.

use std::path::{Path, PathBuf};

static WEIGHTS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/model-q8.bpk"));
static META: &str = include_str!(concat!(env!("OUT_DIR"), "/meta.json"));

/// The embedded weights and meta.json, if this binary was built with a model.
pub fn embedded() -> Option<(&'static [u8], &'static str)> {
    (!WEIGHTS.is_empty() && !META.is_empty()).then_some((WEIGHTS, META))
}

/// Where the model comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelSource {
    /// The int8 model inside this binary.
    Embedded,
    /// A model directory on disk (f32 `model.bpk` + `meta.json`).
    Dir(PathBuf),
}

/// `--model DIR` wins; otherwise the embedded model if there is one; otherwise `default_dir`.
pub fn choose(flag: Option<&Path>, has_embedded: bool, default_dir: &Path) -> ModelSource {
    match flag {
        Some(dir) => ModelSource::Dir(dir.to_path_buf()),
        None if has_embedded => ModelSource::Embedded,
        None => ModelSource::Dir(default_dir.to_path_buf()),
    }
}

/// A short description for the banner and `--help`.
pub fn describe(source: &ModelSource) -> String {
    match source {
        ModelSource::Embedded => {
            format!("embedded int8 model ({} KB)", WEIGHTS.len().div_ceil(1024))
        }
        ModelSource::Dir(dir) => format!("model from {}", dir.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_order() {
        let default = Path::new("data/models/intent");
        let flag = Path::new("/tmp/m");
        assert_eq!(
            choose(Some(flag), true, default),
            ModelSource::Dir(flag.into())
        );
        assert_eq!(
            choose(Some(flag), false, default),
            ModelSource::Dir(flag.into())
        );
        assert_eq!(choose(None, true, default), ModelSource::Embedded);
        assert_eq!(
            choose(None, false, default),
            ModelSource::Dir(default.into())
        );
    }

    #[test]
    fn test_embedded_is_all_or_nothing() {
        // Either both files were embedded or neither: never a half model.
        assert_eq!(WEIGHTS.is_empty(), META.is_empty());
        if let Some((weights, meta)) = embedded() {
            assert!(!weights.is_empty() && meta.contains("\"vocab\""));
        }
    }
}

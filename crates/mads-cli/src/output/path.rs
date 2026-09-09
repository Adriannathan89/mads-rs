//! Package-relative, platform-independent path rendering.

use std::path::Path;

/// Normalizes a source path for the public CLI schema.
///
/// Paths beneath `package_root` are made package-relative, and all separators
/// use `/` so the same record shape is emitted on every supported platform.
/// The function is lexical: it does not require either path to exist.
pub fn normalize_path(package_root: &Path, path: &Path) -> String {
    let package_root = normalize_separators(package_root);
    let path = normalize_separators(path);

    if let Some(relative) = relative_to(&package_root, &path) {
        return relative.to_owned();
    }

    path
}

fn normalize_separators(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn relative_to<'path>(package_root: &str, path: &'path str) -> Option<&'path str> {
    let package_root = package_root.trim_end_matches('/');
    if package_root.is_empty() || path == package_root {
        return (path == package_root).then_some("");
    }
    path.strip_prefix(package_root)
        .and_then(|relative| relative.strip_prefix('/'))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::normalize_path;

    #[test]
    fn normalizes_platform_separators_and_package_relative_paths() {
        assert_eq!(
            normalize_path(
                Path::new("/workspace/app"),
                Path::new("/workspace/app/src/app/routes.rs"),
            ),
            "src/app/routes.rs"
        );
        assert_eq!(
            normalize_path(
                Path::new(r"C:\workspace\app"),
                Path::new(r"C:\workspace\app\src\app\routes.rs"),
            ),
            "src/app/routes.rs"
        );
    }

    #[test]
    fn retains_normalized_paths_outside_the_package_root() {
        assert_eq!(
            normalize_path(
                Path::new("/workspace/app"),
                Path::new("/external/generated.rs"),
            ),
            "/external/generated.rs"
        );
    }
}

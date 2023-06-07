use std::path::{Path, PathBuf};

/// Collapse any '..' in a path and turn it into a relative path (i.e. strip any leading slash).
///
/// If all '..' cannot be collapsed, this function returns `None`.
pub fn normalize(path: &Path) -> Option<PathBuf> {
    let collapsed = path.iter().fold(PathBuf::new(), |mut acc, c| {
        if c == "/" {
            // Skip to avoid making the path absolute.
        } else if c == ".." && acc.parent().is_some() {
            acc.pop();
        } else {
            // This will still accumulate .. at the front of the path,
            // but we deal with this below.
            acc.push(c);
        }

        acc
    });

    assert!(collapsed.is_relative());

    if collapsed.starts_with("..") {
        None
    } else {
        Some(collapsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_normalization() {
        let mut p = PathBuf::new();
        p.push("foo");

        assert!(p.is_relative());

        assert_eq!(normalize(Path::new("")), Some(Path::new("").to_owned()));

        assert_eq!(
            normalize(Path::new("/foo/bar")),
            Some(Path::new("foo/bar").to_owned())
        );

        assert_eq!(normalize(Path::new("../a")), None);

        assert_eq!(
            normalize(Path::new("/foo/../bar/../")),
            Some(Path::new("").to_owned())
        );

        assert_eq!(
            normalize(Path::new("/foo/../bar/../b")),
            Some(Path::new("b").to_owned())
        );
    }
}

use std::path::{Path, PathBuf};

use tokio::fs;

use crate::error::Result;

use super::LibraryCategory;

/// Stable directory names created below the configured Ravyn library root.
pub const LIBRARY_DIRECTORIES: [&str; 10] = [
    "Downloads",
    "Videos",
    "Music",
    "Documents",
    "Images",
    "Archives",
    "Torrents",
    "Playlists",
    "Temporary",
    "Trash",
];

/// Creates the complete library layout without deleting or replacing existing data.
pub async fn prepare_library_layout(root: &Path) -> Result<()> {
    fs::create_dir_all(root).await?;
    for directory in LIBRARY_DIRECTORIES {
        fs::create_dir_all(root.join(directory)).await?;
    }
    Ok(())
}

/// Returns the destination directory for a classified library item.
pub fn category_directory(root: &Path, category: LibraryCategory) -> PathBuf {
    root.join(category.directory_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_the_complete_layout_idempotently() {
        let temporary = tempfile::tempdir().unwrap();
        prepare_library_layout(temporary.path()).await.unwrap();
        prepare_library_layout(temporary.path()).await.unwrap();

        for directory in LIBRARY_DIRECTORIES {
            assert!(temporary.path().join(directory).is_dir(), "{directory}");
        }
    }
}

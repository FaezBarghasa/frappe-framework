use futures::stream::Stream;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, MAIN_SEPARATOR};
use tokio::fs::{create_dir_all, remove_file, rename, File};
use tokio::io::AsyncWriteExt;

#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Directory traversal attempt: {0}")]
    Traversal(String),
    #[error("Stream error: {0}")]
    Stream(String),
}

/// Stores a stream of byte chunks into a content-addressable local storage.
///
/// Algorithmic Complexity: $O(N)$ where $N$ is the stream size in bytes. Single-pass write & hash.
pub async fn store_file_stream<S, E>(
    mut stream: S,
    tenant_id: &str,
    storage_root: &str,
) -> Result<String, StorageError>
where
    S: Stream<Item = Result<Vec<u8>, E>> + Unpin,
    E: std::fmt::Display,
{
    // Block direct path traversal input in tenant_id immediately
    if tenant_id.contains("..") || tenant_id.contains('/') || tenant_id.contains('\\') {
        return Err(StorageError::Traversal("Malformed tenant ID".into()));
    }

    // 1. Setup paths
    let root_path = Path::new(storage_root);
    let tenant_dir = root_path.join(tenant_id);
    create_dir_all(&tenant_dir).await?;
    let canonical_base = tokio::fs::canonicalize(&tenant_dir).await?;

    let tmp_dir = tenant_dir.join("tmp");
    create_dir_all(&tmp_dir).await?;
    let temp_file_path = tmp_dir.join(format!("upload_{}.tmp", uuid::Uuid::new_v4()));

    // 2. Open temp file
    let mut file = File::create(&temp_file_path).await?;
    let mut hasher = Sha256::new();

    // 3. Stream bytes in chunks (64KB chunks typically)
    while let Some(chunk_res) = stream.next().await {
        match chunk_res {
            Ok(chunk) => {
                hasher.update(&chunk);
                if let Err(e) = file.write_all(&chunk).await {
                    // Cleanup partial file to prevent disk leakage on write failure
                    let _ = file.shutdown().await;
                    drop(file);
                    let _ = remove_file(&temp_file_path).await;
                    return Err(StorageError::Io(e));
                }
            }
            Err(e) => {
                // Cleanup partial file on stream error
                let _ = file.shutdown().await;
                drop(file);
                let _ = remove_file(&temp_file_path).await;
                return Err(StorageError::Stream(e.to_string()));
            }
        }
    }

    // Flush and close temp file
    file.flush().await?;
    let _ = file.shutdown().await;
    drop(file);

    // 4. Finalize hash
    let hash_result = hasher.finalize();
    let full_hash = hex::encode(hash_result);
    let hash_prefix = &full_hash[..2];

    // 5. Construct target path and validate boundaries
    let target_dir = tenant_dir.join("public").join("files").join(hash_prefix);
    create_dir_all(&target_dir).await?;
    let canonical_target_dir = tokio::fs::canonicalize(&target_dir).await?;

    // Verify trailing-slash directory boundary checks
    let base_str = canonical_base.to_str().ok_or_else(|| StorageError::Traversal("Invalid base path UTF-8".into()))?;
    let target_str = canonical_target_dir.to_str().ok_or_else(|| StorageError::Traversal("Invalid target path UTF-8".into()))?;

    let mut base_check = base_str.to_string();
    if !base_check.ends_with(MAIN_SEPARATOR) {
        base_check.push(MAIN_SEPARATOR);
    }

    if !target_str.starts_with(&base_check) && target_str != base_str {
        let _ = remove_file(&temp_file_path).await;
        return Err(StorageError::Traversal("Path traversal breakout blocked!".into()));
    }

    let final_file_path = canonical_target_dir.join(&full_hash);

    // Rename temp file to final location
    if let Err(e) = rename(&temp_file_path, &final_file_path).await {
        let _ = remove_file(&temp_file_path).await;
        return Err(StorageError::Io(e));
    }

    Ok(full_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    #[tokio::test]
    async fn test_traversal_blocked() {
        let chunks: Vec<Result<Vec<u8>, std::io::Error>> = vec![Ok(b"hello file data".to_vec())];
        let stream = stream::iter(chunks);

        let res = store_file_stream(stream, "../traversal_tenant", "test_storage").await;
        assert!(res.is_err());
        if let Err(StorageError::Traversal(msg)) = res {
            assert!(msg.contains("Malformed tenant ID") || msg.contains("traversal"));
        } else {
            panic!("Expected traversal error");
        }
    }
}

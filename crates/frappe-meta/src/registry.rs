use crate::schema::DocTypeSchema;
use dashmap::DashMap;
use futures::stream::{FuturesUnordered, StreamExt};
use std::path::PathBuf;
use std::sync::Arc;
use walkdir::WalkDir;

pub struct SchemaRegistry {
    cache: Arc<DashMap<String, DocTypeSchema>>,
}

impl SchemaRegistry {
    /// Create a new schema registry instance.
    pub fn new() -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
        }
    }

    /// Retrieve a schema from cache in $O(1)$ complexity without I/O.
    pub fn get_schema(&self, doctype: &str) -> Option<DocTypeSchema> {
        self.cache.get(doctype).map(|r| r.value().clone())
    }

    /// Retrieve all schemas from cache.
    pub fn get_all_schemas(&self) -> Vec<DocTypeSchema> {
        self.cache.iter().map(|r| r.value().clone()).collect()
    }

    /// Crawls a directory for `.json` schemas in parallel, parsing them using SIMD-JSON.
    pub async fn crawl_and_load_schemas(&self, apps_dir: &str) -> Result<(), String> {
        let mut paths = Vec::new();

        // Find JSON paths recursively using walkdir
        for entry in WalkDir::new(apps_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file()
                && let Some(ext) = entry.path().extension()
                    && ext == "json" {
                        paths.push(entry.into_path());
                    }
        }

        let mut futures = FuturesUnordered::new();
        for path in paths {
            futures.push(async move {
                let data = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
                Ok::<(PathBuf, Vec<u8>), String>((path, data))
            });
        }

        while let Some(res) = futures.next().await {
            match res {
                Ok((path, mut bytes)) => {
                    // simd-json from_slice requires mut slice
                    match simd_json::from_slice::<DocTypeSchema>(&mut bytes) {
                        Ok(schema) => {
                            self.cache.insert(schema.name.clone(), schema);
                        }
                        Err(e) => {
                            log::warn!("Failed to parse schema JSON at {:?}: {:?}", path, e);
                        }
                    }
                }
                Err(e) => {
                    log::error!("File read failed during schema crawl: {:?}", e);
                }
            }
        }

        Ok(())
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

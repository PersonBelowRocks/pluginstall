//! Logic for caching data from APIs. Mainly caching plugin files.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::{Error as IoError, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use derive_new::new;
use http_cache_reqwest::CACacheManager;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Mutex, RwLock};

use crate::adapter::PluginApiType;
use crate::ok_none;

/// The name of the directory where cached data is stored.
pub static DEFAULT_CACHE_DIRECTORY_NAME: &str = ".pluginstall_cache";

/// Name of the cache index file in the cache directory. This file describes where versions of plugins are cached.
pub static CACHE_INDEX_FILE_NAME: &str = "index.json";

/// The name of the directory where cached plugin files are stored.
pub static CACHE_DATA_DIRECTORY_NAME: &str = "data";

/// The name of the (cacache)[https://github.com/zkat/cacache-rs] file in the cache directory.
pub static CACACHE_NAME: &str = "http_cacache";

#[derive(thiserror::Error, Debug)]
pub enum CacheError {
    #[error("IO error '{0}'")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: '{0}'")]
    JsonError(#[from] serde_json::Error),
}

pub type CacheResult<T> = Result<T, CacheError>;

/// Get the default cache directory path, returning an error if it could not be found.
#[inline]
pub fn default_cache_directory_path() -> Result<PathBuf, IoError> {
    let home_dir = homedir::my_home()
        .map_err(|err| IoError::new(ErrorKind::Other, err))?
        .filter(|path| path.exists() && path.is_dir())
        .ok_or_else(|| IoError::new(ErrorKind::Other, "home directory does not exist"))?;

    Ok(home_dir.join(DEFAULT_CACHE_DIRECTORY_NAME))
}

/// Create a cache at the given path. This will initialize the required files and subdirectories for the location
/// to be a valid cache.
#[inline]
pub async fn create_cache(cache_path: &Path) -> Result<(), IoError> {
    tokio::fs::create_dir_all(cache_path).await?;

    let index_file_path = cache_path.join(CACHE_INDEX_FILE_NAME);
    // create the index file with an empty map
    if !index_file_path.exists() || !index_file_path.is_file() {
        File::create(index_file_path)
            .await?
            .write_all("{}".as_bytes())
            .await?;
    }

    let data_dir_path = cache_path.join(CACHE_DATA_DIRECTORY_NAME);
    // create the data directory
    if !data_dir_path.exists() || !data_dir_path.is_dir() {
        tokio::fs::create_dir(data_dir_path).await?;
    }

    Ok(())
}

/// Compute the name of a file with cached data of a plugin.
#[inline]
fn compute_cache_file_name(
    plugin_name: &str,
    version_identifier: &str,
    plugin_type: PluginApiType,
) -> String {
    format!("{plugin_type}-{plugin_name}-{version_identifier}.CACHED")
}

/// Representation of the cache on disk. Supports various cache operations.
#[derive(Debug)]
pub struct DownloadCache {
    cache_path: PathBuf,
    cache_index_path: PathBuf,
    cache_datadir_path: PathBuf,
    /// The deserialized cache index from the index file.
    cache_index: RwLock<CacheIndex>,
}

#[allow(dead_code)]
impl DownloadCache {
    /// Create a new handle to cache at the given path.
    /// Will return an error if the cache is not present or has an invalid structure.
    #[inline]
    pub async fn new(cache_path: &Path) -> CacheResult<Self> {
        // ensure this path points to a directory
        if !cache_path.exists() || !cache_path.is_dir() {
            log::error!("Invalid cache path: {}", cache_path.to_string_lossy());
            return Err(IoError::new(ErrorKind::Other, "invalid cache").into());
        }

        let data_path = cache_path.join(CACHE_DATA_DIRECTORY_NAME);
        // ensure that the data directory exists
        if !data_path.exists() || !data_path.is_dir() {
            log::error!("Invalid cache data dir: {}", data_path.to_string_lossy());
            return Err(IoError::new(ErrorKind::Other, "invalid cache").into());
        }

        let index_path = cache_path.join(CACHE_INDEX_FILE_NAME);
        let mut index_file = File::open(&index_path).await.inspect_err(|_| {
            log::error!("Invalid cache index file: {}", index_path.to_string_lossy())
        })?;

        let mut cache_file_contents = String::new();
        index_file.read_to_string(&mut cache_file_contents).await?;
        index_file.rewind().await?;

        let cache_index = serde_json::from_str::<CacheIndex>(&cache_file_contents)?;

        Ok(Self {
            cache_path: cache_path.to_path_buf(),
            cache_index_path: index_path,
            cache_datadir_path: data_path,

            cache_index: RwLock::new(cache_index),
        })
    }

    /// Get the cache manager for caching general HTTP requests.
    #[inline]
    pub fn cacache_manager(&self) -> CACacheManager {
        CACacheManager {
            path: self.cache_path.join(CACACHE_NAME),
        }
    }

    /// Attempt to open the cache index file on the disk.
    #[inline]
    async fn open_and_clear_index_file(&self) -> CacheResult<File> {
        OpenOptions::new()
            .truncate(true)
            .write(true)
            .create(true)
            .open(&self.cache_index_path)
            .await
            .map_err(CacheError::from)
    }

    /// Sync the in-memory cache index to the cache index file on disk.
    /// This will clear existing data in the cache index file and overwrite it with the in-memory data.
    #[inline]
    async fn sync_index_to_fs(&self) -> CacheResult<()> {
        let index = self.cache_index.read().await;
        let json = serde_json::to_string_pretty(&*index)?; // pretty format so its somewhat human readable

        let mut index_file = self.open_and_clear_index_file().await?;

        index_file.write_all(json.as_bytes()).await?;
        index_file.flush().await?;
        index_file.sync_all().await?;

        Ok(())
    }

    /// Get metadata of a cached version of a plugin from the index.
    #[inline]
    async fn get_cached_plugin_metadata(
        &self,
        plugin_name: &str,
        version_identifier: &str,
    ) -> Option<CacheIndexFile> {
        let cache_index = self.cache_index.read().await;

        cache_index
            .plugins
            .get(plugin_name)?
            .versions
            .get(version_identifier)
            .cloned()
    }

    /// Delete a cached plugin and returns its metadata (if it existed and was deleted).
    ///
    /// Returns `Ok(CacheIndexFile)` if the cached plugin version existed in the cache and was successfully deleted.
    /// Returns `Ok(None)` if the plugin version did not exist in the cache and was therefore not deleted.
    /// Returns `Err()` otherwise if there was an error.
    #[inline]
    pub async fn delete_cached_file(
        &self,
        plugin_name: &str,
        version_identifier: &str,
    ) -> CacheResult<Option<CacheIndexFile>> {
        let mut cache_index = self.cache_index.write().await;

        let Entry::Occupied(mut plugin_entry) = cache_index.plugins.entry(plugin_name.to_string())
        else {
            return Ok(None);
        };

        let removed = ok_none!(plugin_entry.get_mut().versions.remove(version_identifier));

        // remove the entire plugin entry in the index if it has no files
        if plugin_entry.get().versions.is_empty() {
            plugin_entry.remove_entry();
        }

        // remove the cached file
        let cached_file_path = self.cache_datadir_path.join(&removed.file_name);
        tokio::fs::remove_file(cached_file_path).await?;

        Ok(Some(removed))
    }

    /// Get and open the cached plugin version if it exists.
    /// Returns [`None`] if this version was not cached.
    #[inline]
    pub async fn get_cached_file(
        &self,
        plugin_name: &str,
        version_identifier: &str,
    ) -> CacheResult<Option<CachedFile>> {
        let meta = ok_none!(
            self.get_cached_plugin_metadata(plugin_name, version_identifier)
                .await
        );

        // if the retrieved file is outdated, then delete it and claim it never existed.
        // cached data is only valid as long as it's up to date
        if meta.is_outdated() {
            self.delete_cached_file(plugin_name, version_identifier)
                .await?;
            return Ok(None);
        }

        let file_path = self.cache_datadir_path.join(&meta.cache_file_name);
        let file = File::open(&file_path).await?;

        Ok(Some(CachedFile { meta, file }))
    }

    /// Cache the data from the given reader.
    /// An entry will be created in the index with the provided `plugin_name`, `version_identifier`, `file_name`, `plugin_type`, and `ttl`.
    /// Addtionally, the current (local) datetime will be added to the entry as the date when this cache entry was created.
    #[inline]
    pub async fn cache_file(
        &self,
        plugin_name: &str,
        version_identifier: &str,
        file_name: &str,
        plugin_type: PluginApiType,
        ttl: Option<chrono::Duration>,
        data: &[u8],
    ) -> CacheResult<()> {
        let mut index = self.cache_index.write().await;

        let plugins = index
            .plugins
            .entry(plugin_name.to_string())
            .or_insert_with(|| CacheIndexPlugin::new(plugin_type));

        let cache_file_name = compute_cache_file_name(plugin_name, version_identifier, plugin_type);
        let cache_file_path = self.cache_datadir_path.join(&cache_file_name);

        let mut file = File::create(&cache_file_path).await?;

        file.write_all(data).await?;
        file.flush().await?;
        file.sync_all().await?; // TODO: do we need to both flush and sync_all here? or is one of them enough

        let cache_index_file = CacheIndexFile {
            // current localtime
            added: chrono::Local::now().to_utc(),
            file_name: file_name.to_string(),
            cache_file_name,
            ttl,
        };

        plugins
            .versions
            .insert(version_identifier.to_string(), cache_index_file);

        // finally make sure that the index is accurately represented on disk.
        self.sync_index_to_fs().await?;

        Ok(())
    }
}

/// A cached plugin file.
#[derive(Debug)]
pub struct CachedFile {
    /// Metadata of the cached file.
    pub meta: CacheIndexFile,
    /// Handle to the cached file's data.
    pub file: File,
}

/// The cache index. Deserialized from (and serialized to) the cache index file (`index.json`).
///
/// Use this to find which file contains the cached data for a version of a plugin.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CacheIndex {
    /// Maps the manifest name of plugins to their cached files.
    #[serde(default)]
    pub plugins: HashMap<String, CacheIndexPlugin>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, new)]
pub struct CacheIndexPlugin {
    /// Cached versions of this resource.
    /// Maps a version identifier to a cached file.
    #[new(default)]
    pub versions: HashMap<String, CacheIndexFile>,
    /// The API this plugin was sourced from.
    pub source_api: PluginApiType,
}

/// A cached downloaded plugin.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CacheIndexFile {
    /// The original plugin's file name.
    pub file_name: String,
    /// The file name of the cached plugin in the cache data directory.
    pub cache_file_name: String,
    /// The TTL (if any) of this cached file.
    pub ttl: Option<chrono::Duration>,
    /// The date that this file was added to the cache.
    pub added: chrono::DateTime<Utc>,
}

impl CacheIndexFile {
    /// Returns whether this file has outlived its TTL (if it has a TTL).
    ///
    /// Returns `true` if it's outdated.
    /// Returns `false` if it's not outdated or if it doesn't have a TTL.
    #[inline]
    pub fn is_outdated(&self) -> bool {
        match self.ttl {
            Some(ttl) => {
                let localtime = chrono::Local::now().to_utc();
                let Some(expiry_datetime) = self.added.checked_add_signed(ttl) else {
                    // TODO: maybe do something more here lol it feels like overflowing the datetime should be a bigger deal?
                    //  also should we even return true to begin with? we're essentially marking this file for deletion...
                    return true;
                };

                localtime <= expiry_datetime
            }
            None => false,
        }
    }
}

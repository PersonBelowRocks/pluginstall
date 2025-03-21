//! Logic for caching data from APIs. Mainly caching plugin files.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use chrono::Utc;
use derive_new::new;
use directories::UserDirs;
use http_cache_reqwest::CACacheManager;
use tokio::fs::{self, File};
use tokio::io::{self, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::RwLock;

use crate::adapter::PluginApiType;
use crate::error::ParseError;
use crate::ok_none;

/// The name of the directory where cached data is stored.
pub static DEFAULT_CACHE_DIRECTORY_NAME: &str = ".pluginstall_cache";

/// Name of the cache index file in the cache directory. This file describes where versions of plugins are cached.
pub static CACHE_INDEX_FILE_NAME: &str = "index.json";

/// The name of the directory where cached plugin files are stored.
pub static CACHE_DATA_DIRECTORY_NAME: &str = "data";

/// The name of the (cacache)[https://github.com/zkat/cacache-rs] file in the cache directory.
pub static CACACHE_NAME: &str = "http_cacache";

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum CacheError {
    #[error(transparent)]
    Io(#[from] io::Error),
    /// An error serializing/deserializing the cache index
    #[error(transparent)]
    IndexParse(ParseError),
    #[error("Error copying cached plugin file: {0}")]
    CopyFile(io::Error),
}

pub type CacheResult<T> = Result<T, CacheError>;

/// Get the default cache directory path, returning an error if it could not be found.
#[inline]
pub fn default_cache_directory_path() -> io::Result<PathBuf> {
    let dirs = UserDirs::new().ok_or(io::Error::other("could not get home directory"))?;
    let home_dir = dirs.home_dir();

    Ok(home_dir.join(DEFAULT_CACHE_DIRECTORY_NAME))
}

/// Create a cache at the given path. This will initialize the required files and subdirectories for the location
/// to be a valid cache.
#[inline]
pub async fn create_cache(cache_path: &Path) -> io::Result<()> {
    fs::create_dir_all(cache_path).await?;

    let index_file_path = cache_path.join(CACHE_INDEX_FILE_NAME);
    // create the index file with an empty map
    if !index_file_path.is_file() {
        File::create(index_file_path)
            .await?
            .write_all("{}".as_bytes())
            .await?;
    }

    let data_dir_path = cache_path.join(CACHE_DATA_DIRECTORY_NAME);
    // create the data directory
    if !data_dir_path.is_dir() {
        fs::create_dir(data_dir_path).await?;
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
        let data_path = cache_path.join(CACHE_DATA_DIRECTORY_NAME);
        // ensure that the data directory exists
        if !data_path.is_dir() {
            fs::create_dir(&data_path).await?;
        }

        let index_file_path = cache_path.join(CACHE_INDEX_FILE_NAME);
        let cache_index = match CacheIndex::open(&index_file_path).await {
            // try to create a cache index if one doesn't exist
            Err(IndexError::Io(err)) if matches!(err.kind(), ErrorKind::NotFound) => {
                CacheIndex::create_in_dir(cache_path).await?
            }
            Err(err) => {
                return Err(match err {
                    IndexError::Io(error) => CacheError::Io(error),
                    IndexError::Parse(error) => CacheError::IndexParse(error),
                })
            }
            Ok(index) => index,
        };

        Ok(Self {
            cache_path: cache_path.to_path_buf(),
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

    /// Get metadata of a cached version of a plugin from the index.
    #[inline]
    async fn get_cached_plugin_metadata(
        &self,
        plugin_name: &str,
        version_identifier: &str,
    ) -> Option<CachedPluginVersionFile> {
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
    ) -> CacheResult<Option<CachedPluginVersionFile>> {
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
        let cached_file_path = self.cache_datadir_path.join(&removed.cache_file_name);
        fs::remove_file(cached_file_path).await?;

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
            .or_insert_with(|| CachedPlugin::new(plugin_type));

        let cache_file_name = compute_cache_file_name(plugin_name, version_identifier, plugin_type);
        let cache_file_path = self.cache_datadir_path.join(&cache_file_name);

        let mut file = File::create(&cache_file_path).await?;
        file.write_all(data).await?;
        file.flush().await?;

        let cache_index_file = CachedPluginVersionFile {
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
        index.sync_to_disk().await?;

        Ok(())
    }
}

/// A cached plugin file.
#[derive(Debug)]
pub struct CachedFile {
    /// Metadata of the cached file.
    pub meta: CachedPluginVersionFile,
    /// Handle to the cached file's data.
    pub file: File,
}

impl CachedFile {
    /// Copy this cached file to the given directory, with the original name of the downloaded file.
    /// Returns the number of bytes copied (i.e., the size of the file).
    #[inline]
    pub async fn copy_to_directory(&mut self, dir: &Path) -> CacheResult<u64> {
        let out_file_path = dir.join(&self.meta.file_name);
        let mut out_file = File::create(&out_file_path)
            .await
            .map_err(CacheError::CopyFile)?;

        let copied = io::copy(&mut self.file, &mut out_file)
            .await
            .map_err(CacheError::CopyFile)?;

        self.file.rewind().await.map_err(CacheError::CopyFile)?; // rewind so future uses of this object will behave nicely
        out_file.flush().await.map_err(CacheError::CopyFile)?; // flush the data to disk

        Ok(copied)
    }
}

/// The cache index.
///
/// Use this to find which file contains the cached data for a version of a plugin.
#[derive(Debug)]
pub struct CacheIndex {
    /// The path to the index on disk.
    pub path: PathBuf,
    /// Maps the manifest name of plugins to their cached files.
    /// Deserialized from (and serialized to) the cache index file ([`IndexFile::path`])
    pub plugins: IndexFilePlugins,
}

/// The plugins in an index file.
pub type IndexFilePlugins = HashMap<String, CachedPlugin>;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, new)]
pub struct CachedPlugin {
    /// Cached versions of this resource.
    /// Maps a version identifier to a cached file.
    #[new(default)]
    pub versions: HashMap<String, CachedPluginVersionFile>,
    /// The API this plugin was sourced from.
    pub source_api: PluginApiType,
}

/// A cached downloaded plugin.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CachedPluginVersionFile {
    /// The original plugin's file name.
    pub file_name: String,
    /// The file name of the cached plugin in the cache data directory.
    pub cache_file_name: String,
    /// The TTL (if any) of this cached file.
    pub ttl: Option<chrono::Duration>,
    /// The date that this file was added to the cache.
    pub added: chrono::DateTime<Utc>,
}

/// An error serializing/deserializing the cache index.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum IndexError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

impl CacheIndex {
    /// Create a new index file in the given directory, overwriting any existing file named `index.json`.
    #[inline]
    pub async fn create_in_dir(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();

        let new = Self {
            path: path.join(CACHE_INDEX_FILE_NAME),
            plugins: IndexFilePlugins::default(),
        };

        // create/overwrite the index file
        File::create(&new.path).await?;

        // do an initial sync to populate the index file
        new.sync_to_disk().await?;

        Ok(new)
    }

    /// Open a cache index on disk.
    #[inline]
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, IndexError> {
        let path = path.as_ref();
        let mut cache_index_file = File::open(path).await?;

        let mut contents = String::new();
        cache_index_file.read_to_string(&mut contents).await?;

        Ok(Self {
            path: path.to_path_buf(),
            plugins: serde_json::from_str(&contents)
                .map_err(|err| ParseError::json(err, contents))?,
        })
    }

    /// Sync this cache index to disk.
    #[inline]
    pub async fn sync_to_disk(&self) -> io::Result<()> {
        let json = serde_json::to_string_pretty(&self.plugins)
            .expect("the serialize implementation is derived and shouldn't fail");

        let mut file = File::open(&self.path).await?;
        file.write_all(json.as_bytes()).await?;
        file.flush().await?;

        Ok(())
    }
}

impl CachedPluginVersionFile {
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

                localtime >= expiry_datetime
            }
            None => false,
        }
    }
}

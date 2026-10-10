use std::borrow::Cow;

use base64::Engine as _;
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

const VALUE_FORMAT_VERSION: u32 = 1;
const MAX_KEY_BYTES: usize = 180;

/// Errors returned by local key-value storage.
#[derive(Debug, Error)]
pub enum DataStoreError {
    /// The application or store name is not a valid single path component.
    #[error(
        "invalid {kind}: use a non-empty ASCII name containing only letters, numbers, '-' or '_'"
    )]
    InvalidName {
        /// Identifies which configured name was invalid.
        kind: &'static str,
    },
    /// A key was empty or too large to use as a portable filename.
    #[error("data store key must contain between 1 and {MAX_KEY_BYTES} UTF-8 bytes")]
    InvalidKey,
    /// The stored value was written using a newer data-store envelope format.
    #[error("unsupported data store value format version {0}")]
    UnsupportedFormatVersion(u32),
    /// A RenFS filesystem operation failed.
    #[error("data store filesystem error: {0}")]
    Filesystem(#[from] renfs::Error),
    /// A value could not be encoded or decoded as JSON.
    #[error("data store value serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// A const-constructible handle to one local, named key-value store.
///
/// This is only a descriptor; it does not retain an open filesystem handle or
/// perform I/O during construction. Store values are written beneath the
/// application directory returned by RenFS. Keys are stored as separate files
/// and may contain any UTF-8 text up to 180 bytes.
///
/// ```no_run
/// use terrarium::LocalDataStore;
///
/// static PLAYER_DATA: LocalDataStore =
///     LocalDataStore::new("my-game", "players");
///
/// # async fn example() -> Result<(), terrarium::DataStoreError> {
/// #[derive(serde::Serialize, serde::Deserialize)]
/// struct Profile {
///     coins: u32,
/// }
///
/// let profile: Option<Profile> = PLAYER_DATA.get("user:42").await?;
/// if let Some(profile) = profile {
///     PLAYER_DATA.set("user:42", &profile).await?;
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalDataStore {
    app_name: Cow<'static, str>,
    store_name: Cow<'static, str>,
}

impl LocalDataStore {
    /// Creates a static-friendly local store descriptor.
    ///
    /// `app_name` selects the RenFS application directory; `store_name`
    /// separates this store from other stores belonging to the same game.
    /// Both names are validated when an operation is performed.
    pub const fn new(app_name: &'static str, store_name: &'static str) -> Self {
        Self {
            app_name: Cow::Borrowed(app_name),
            store_name: Cow::Borrowed(store_name),
        }
    }

    /// Creates a store descriptor from dynamically owned names.
    ///
    /// Use [`Self::new`] when the names are known at compile time and the
    /// descriptor should be usable from a `const` or `static`.
    pub fn from_owned(app_name: String, store_name: String) -> Self {
        Self {
            app_name: Cow::Owned(app_name),
            store_name: Cow::Owned(store_name),
        }
    }

    /// Reads and deserializes the value for `key`, or returns `None` if it is absent.
    pub async fn get<T>(&self, key: &str) -> Result<Option<T>, DataStoreError>
    where
        T: DeserializeOwned,
    {
        let path = self.value_path(key)?;
        let directory = self.directory().await?;
        if !directory.exists(&path).await? {
            return Ok(None);
        }

        let bytes = directory.read_file(&path).await?;
        let entry: StoredValue = serde_json::from_slice(&bytes)?;
        if entry.format_version != VALUE_FORMAT_VERSION {
            return Err(DataStoreError::UnsupportedFormatVersion(
                entry.format_version,
            ));
        }

        Ok(Some(serde_json::from_value(entry.value)?))
    }

    /// Serializes and replaces the value for `key`.
    ///
    /// Concurrent writes to the same key are last-write-wins. This operation
    /// does not provide cross-tab or cross-process transaction guarantees.
    pub async fn set<T>(&self, key: &str, value: &T) -> Result<(), DataStoreError>
    where
        T: Serialize + ?Sized,
    {
        let path = self.value_path(key)?;
        let directory = self.directory().await?;
        let entry = StoredValue {
            format_version: VALUE_FORMAT_VERSION,
            value: serde_json::to_value(value)?,
        };
        let bytes = serde_json::to_vec(&entry)?;
        directory.write_file(&path, &bytes).await?;
        Ok(())
    }

    /// Deletes `key`, returning whether a value existed.
    pub async fn delete(&self, key: &str) -> Result<bool, DataStoreError> {
        let path = self.value_path(key)?;
        let directory = self.directory().await?;
        if !directory.exists(&path).await? {
            return Ok(false);
        }

        directory.unlink(&path).await?;
        Ok(true)
    }

    async fn directory(&self) -> Result<renfs::Directory, DataStoreError> {
        validate_name(&self.app_name, "application name")?;
        validate_name(&self.store_name, "store name")?;

        let directory = renfs::app_dir(&self.app_name)?;
        directory.mkdir(&self.store_name, true).await?;
        Ok(directory)
    }

    fn value_path(&self, key: &str) -> Result<String, DataStoreError> {
        validate_key(key)?;
        let encoded_key = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key);
        Ok(format!("{}/{encoded_key}.json", self.store_name))
    }
}

#[derive(Serialize, serde::Deserialize)]
struct StoredValue {
    format_version: u32,
    value: serde_json::Value,
}

fn validate_name(name: &str, kind: &'static str) -> Result<(), DataStoreError> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(DataStoreError::InvalidName { kind });
    }
    Ok(())
}

fn validate_key(key: &str) -> Result<(), DataStoreError> {
    if key.is_empty() || key.len() > MAX_KEY_BYTES {
        return Err(DataStoreError::InvalidKey);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    static NEXT_TEST_APP: AtomicUsize = AtomicUsize::new(0);

    struct TestAppDirectory(std::path::PathBuf);

    impl Drop for TestAppDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn typed_values_can_be_read_replaced_and_deleted_from_the_local_store() {
        #[derive(Debug, PartialEq, Serialize, serde::Deserialize)]
        struct Profile {
            coins: u32,
        }

        let id = NEXT_TEST_APP.fetch_add(1, Ordering::Relaxed);
        let app_name = format!("terrarium-test-{}-{id}", std::process::id());
        let store = LocalDataStore::from_owned(app_name.clone(), "profiles".to_owned());
        let app_directory = renfs::app_dir(&app_name).unwrap();
        let _cleanup = TestAppDirectory(app_directory.as_path().into());

        pollster::block_on(async {
            let key = "user/42";
            assert_eq!(store.get::<Profile>(key).await.unwrap(), None);

            store.set(key, &Profile { coins: 125 }).await.unwrap();
            assert_eq!(
                store.get::<Profile>(key).await.unwrap(),
                Some(Profile { coins: 125 })
            );

            store.set(key, &Profile { coins: 200 }).await.unwrap();
            assert_eq!(
                store.get::<Profile>(key).await.unwrap(),
                Some(Profile { coins: 200 })
            );

            assert!(store.delete(key).await.unwrap());
            assert!(!store.delete(key).await.unwrap());
            assert_eq!(store.get::<Profile>(key).await.unwrap(), None);
        });
    }
}

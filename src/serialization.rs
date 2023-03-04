use std::{fs, path::Path};

pub trait Serializer {
    /// Configuration and credential file helper
    /// Creates a default configuration if none exist, otherwise will optionally overwrite
    /// the file if it fails to parse
    fn load_or_generate_default<
        P: AsRef<Path>,
        T: serde::Serialize + serde::de::DeserializeOwned,
        F: Fn() -> Result<T, String>,
    >(
        &self,
        path: P,
        default: F,
        default_on_parse_failure: bool,
    ) -> Result<T, String> {
        let path = path.as_ref();
        // Nothing exists so just write the default and return it
        if !path.exists() {
            let value = default()?;
            return self.write(path, value);
        }

        let result = self.load(path);
        if default_on_parse_failure && result.is_err() {
            let value = default()?;
            return self.write(path, value);
        }
        result.map_err(|e| format!("Unable to parse {}: {}", path.to_string_lossy(), e))
    }

    fn load<P: AsRef<Path>, T: serde::Serialize + serde::de::DeserializeOwned>(
        &self,
        path: P,
    ) -> Result<T, String>;
    fn write<P: AsRef<Path>, T: serde::Serialize>(&self, path: P, value: T) -> Result<T, String>;
}

pub struct TomlSerializer {}
impl Serializer for TomlSerializer {
    fn load<P: AsRef<Path>, T: serde::Serialize + serde::de::DeserializeOwned>(
        &self,
        path: P,
    ) -> Result<T, String> {
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| format!("Unable to read {}: {}", path.as_ref().to_string_lossy(), e))?;
        toml::from_str(&contents).map_err(|e| {
            format!(
                "Unable to parse toml {}: {}",
                path.as_ref().to_string_lossy(),
                e
            )
        })
    }

    fn write<P: AsRef<Path>, T: serde::Serialize>(&self, path: P, value: T) -> Result<T, String> {
        let content =
            toml::to_string_pretty(&value).map_err(|e| format!("Failed serializing value: {e}"))?;
        fs::write(path.as_ref(), content)
            .map(|_| value)
            .map_err(|e| {
                format!(
                    "Failed writing content to {}: {}",
                    path.as_ref().display(),
                    e
                )
            })
    }
}

pub struct CborSerializer {}
impl Serializer for CborSerializer {
    fn load<P: AsRef<Path>, T: serde::Serialize + serde::de::DeserializeOwned>(
        &self,
        path: P,
    ) -> Result<T, String> {
        let contents = std::fs::read(&path)
            .map_err(|e| format!("Unable to read {}: {}", path.as_ref().to_string_lossy(), e))?;
        serde_cbor::from_slice(&contents).map_err(|e| {
            format!(
                "Unable to parse CBOR {}: {}",
                path.as_ref().to_string_lossy(),
                e
            )
        })
    }

    fn write<P: AsRef<Path>, T: serde::Serialize>(&self, path: P, value: T) -> Result<T, String> {
        let file = std::fs::File::create(&path).map_err(|e| {
            format!(
                "Failed creating file {}: {}",
                path.as_ref().to_string_lossy(),
                e
            )
        })?;
        serde_cbor::to_writer(file, &value)
            .map(|_| value)
            .map_err(|e| {
                format!(
                    "Failed writing content to {}: {}",
                    path.as_ref().display(),
                    e
                )
            })
    }
}

pub static TOML: TomlSerializer = TomlSerializer {};
pub static CBOR: CborSerializer = CborSerializer {};

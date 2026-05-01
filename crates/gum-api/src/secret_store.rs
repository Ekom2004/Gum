use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::{STANDARD as B64_STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use postgres::{Client, NoTls};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};

const SECRET_BACKEND_MEMORY: &str = "memory";
const SECRET_BACKEND_POSTGRES: &str = "postgres_aes256_gcm_v1";
const SECRET_TABLE_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS project_secrets (
    project_id TEXT NOT NULL,
    environment TEXT NOT NULL,
    name TEXT NOT NULL,
    nonce_b64 TEXT NOT NULL,
    ciphertext_b64 TEXT NOT NULL,
    key_version INTEGER NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ NULL,
    PRIMARY KEY (project_id, environment, name)
);
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretMetadata {
    pub project_id: String,
    pub environment: String,
    pub name: String,
    pub backend: String,
    pub updated_at_epoch_ms: i64,
    pub last_used_at_epoch_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetSecretParams {
    pub project_id: String,
    pub environment: String,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveSecretParams {
    pub project_id: String,
    pub environment: String,
    pub name: String,
}

pub trait SecretStore: Send + Sync {
    fn set_secret(&self, params: SetSecretParams) -> Result<SecretMetadata, String>;
    fn list_secrets(
        &self,
        project_id: &str,
        environment: &str,
    ) -> Result<Vec<SecretMetadata>, String>;
    fn delete_secret(
        &self,
        project_id: &str,
        environment: &str,
        name: &str,
    ) -> Result<bool, String>;
    fn resolve_secret(&self, params: ResolveSecretParams) -> Result<Option<String>, String>;
}

#[derive(Debug, Default)]
pub struct InMemorySecretStore {
    inner: Mutex<HashMap<SecretKey, StoredSecret>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SecretKey {
    project_id: String,
    environment: String,
    name: String,
}

#[derive(Debug, Clone)]
struct StoredSecret {
    value: String,
    metadata: SecretMetadata,
}

#[derive(Debug, Clone)]
pub struct PostgresSecretStore {
    database_url: Arc<String>,
    cipher: SecretCipher,
}

#[derive(Debug, Clone)]
struct SecretCipher {
    key: [u8; 32],
    key_version: i32,
}

#[derive(Debug, Clone)]
struct EncryptedSecret {
    nonce_b64: String,
    ciphertext_b64: String,
}

impl InMemorySecretStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl SecretStore for InMemorySecretStore {
    fn set_secret(&self, params: SetSecretParams) -> Result<SecretMetadata, String> {
        let now = now_epoch_ms();
        let key = SecretKey {
            project_id: params.project_id.clone(),
            environment: params.environment.clone(),
            name: params.name.clone(),
        };
        let metadata = SecretMetadata {
            project_id: params.project_id,
            environment: params.environment,
            name: params.name,
            backend: SECRET_BACKEND_MEMORY.to_string(),
            updated_at_epoch_ms: now,
            last_used_at_epoch_ms: None,
        };
        let stored = StoredSecret {
            value: params.value,
            metadata: metadata.clone(),
        };
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "secret store mutex poisoned".to_string())?;
        state.insert(key, stored);
        Ok(metadata)
    }

    fn list_secrets(
        &self,
        project_id: &str,
        environment: &str,
    ) -> Result<Vec<SecretMetadata>, String> {
        let state = self
            .inner
            .lock()
            .map_err(|_| "secret store mutex poisoned".to_string())?;
        let mut out: Vec<SecretMetadata> = state
            .values()
            .filter(|entry| {
                entry.metadata.project_id == project_id && entry.metadata.environment == environment
            })
            .map(|entry| entry.metadata.clone())
            .collect();
        out.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(out)
    }

    fn delete_secret(
        &self,
        project_id: &str,
        environment: &str,
        name: &str,
    ) -> Result<bool, String> {
        let key = SecretKey {
            project_id: project_id.to_string(),
            environment: environment.to_string(),
            name: name.to_string(),
        };
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "secret store mutex poisoned".to_string())?;
        Ok(state.remove(&key).is_some())
    }

    fn resolve_secret(&self, params: ResolveSecretParams) -> Result<Option<String>, String> {
        let key = SecretKey {
            project_id: params.project_id,
            environment: params.environment,
            name: params.name,
        };
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "secret store mutex poisoned".to_string())?;
        let Some(entry) = state.get_mut(&key) else {
            return Ok(None);
        };
        entry.metadata.last_used_at_epoch_ms = Some(now_epoch_ms());
        Ok(Some(entry.value.clone()))
    }
}

impl PostgresSecretStore {
    pub fn connect(database_url: &str, master_key: [u8; 32]) -> Result<Self, String> {
        let mut client = Client::connect(database_url, NoTls)
            .map_err(|error| format!("failed to connect to postgres for secret store: {error}"))?;
        client
            .batch_execute(SECRET_TABLE_DDL)
            .map_err(|error| format!("failed to apply secret store schema: {error}"))?;

        Ok(Self {
            database_url: Arc::new(database_url.to_string()),
            cipher: SecretCipher {
                key: master_key,
                key_version: 1,
            },
        })
    }

    fn connect_client(&self) -> Result<Client, String> {
        Client::connect(self.database_url.as_str(), NoTls)
            .map_err(|error| format!("failed to connect to postgres for secret store: {error}"))
    }
}

impl SecretStore for PostgresSecretStore {
    fn set_secret(&self, params: SetSecretParams) -> Result<SecretMetadata, String> {
        let encrypted = self.cipher.encrypt(&params.value)?;
        let mut client = self.connect_client()?;
        client
            .execute(
                "INSERT INTO project_secrets (
                    project_id, environment, name, nonce_b64, ciphertext_b64, key_version, updated_at, last_used_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, NOW(), NULL)
                 ON CONFLICT (project_id, environment, name) DO UPDATE
                 SET nonce_b64 = EXCLUDED.nonce_b64,
                     ciphertext_b64 = EXCLUDED.ciphertext_b64,
                     key_version = EXCLUDED.key_version,
                     updated_at = NOW(),
                     last_used_at = NULL",
                &[
                    &params.project_id,
                    &params.environment,
                    &params.name,
                    &encrypted.nonce_b64,
                    &encrypted.ciphertext_b64,
                    &self.cipher.key_version,
                ],
            )
            .map_err(|error| format!("failed to set secret: {error}"))?;

        Ok(SecretMetadata {
            project_id: params.project_id,
            environment: params.environment,
            name: params.name,
            backend: SECRET_BACKEND_POSTGRES.to_string(),
            updated_at_epoch_ms: now_epoch_ms(),
            last_used_at_epoch_ms: None,
        })
    }

    fn list_secrets(
        &self,
        project_id: &str,
        environment: &str,
    ) -> Result<Vec<SecretMetadata>, String> {
        let mut client = self.connect_client()?;
        let rows = client
            .query(
                "SELECT project_id,
                        environment,
                        name,
                        (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint AS updated_at_epoch_ms,
                        CASE
                            WHEN last_used_at IS NULL THEN NULL
                            ELSE (EXTRACT(EPOCH FROM last_used_at) * 1000)::bigint
                        END AS last_used_at_epoch_ms
                 FROM project_secrets
                 WHERE project_id = $1 AND environment = $2
                 ORDER BY name",
                &[&project_id, &environment],
            )
            .map_err(|error| format!("failed to list secrets: {error}"))?;

        rows.into_iter()
            .map(|row| {
                Ok(SecretMetadata {
                    project_id: row
                        .try_get("project_id")
                        .map_err(|error| format!("failed reading secret project_id: {error}"))?,
                    environment: row
                        .try_get("environment")
                        .map_err(|error| format!("failed reading secret environment: {error}"))?,
                    name: row
                        .try_get("name")
                        .map_err(|error| format!("failed reading secret name: {error}"))?,
                    backend: SECRET_BACKEND_POSTGRES.to_string(),
                    updated_at_epoch_ms: row
                        .try_get("updated_at_epoch_ms")
                        .map_err(|error| format!("failed reading secret updated_at: {error}"))?,
                    last_used_at_epoch_ms: row
                        .try_get("last_used_at_epoch_ms")
                        .map_err(|error| format!("failed reading secret last_used_at: {error}"))?,
                })
            })
            .collect()
    }

    fn delete_secret(
        &self,
        project_id: &str,
        environment: &str,
        name: &str,
    ) -> Result<bool, String> {
        let mut client = self.connect_client()?;
        let deleted = client
            .execute(
                "DELETE FROM project_secrets
                 WHERE project_id = $1 AND environment = $2 AND name = $3",
                &[&project_id, &environment, &name],
            )
            .map_err(|error| format!("failed to delete secret: {error}"))?;
        Ok(deleted > 0)
    }

    fn resolve_secret(&self, params: ResolveSecretParams) -> Result<Option<String>, String> {
        let mut client = self.connect_client()?;
        let row = client
            .query_opt(
                "SELECT nonce_b64, ciphertext_b64
                 FROM project_secrets
                 WHERE project_id = $1 AND environment = $2 AND name = $3",
                &[&params.project_id, &params.environment, &params.name],
            )
            .map_err(|error| format!("failed to resolve secret metadata: {error}"))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let nonce_b64: String = row
            .try_get("nonce_b64")
            .map_err(|error| format!("failed reading secret nonce: {error}"))?;
        let ciphertext_b64: String = row
            .try_get("ciphertext_b64")
            .map_err(|error| format!("failed reading secret ciphertext: {error}"))?;

        let value = self.cipher.decrypt(&nonce_b64, &ciphertext_b64)?;
        client
            .execute(
                "UPDATE project_secrets
                 SET last_used_at = NOW()
                 WHERE project_id = $1 AND environment = $2 AND name = $3",
                &[&params.project_id, &params.environment, &params.name],
            )
            .map_err(|error| format!("failed to update secret last_used_at: {error}"))?;
        Ok(Some(value))
    }
}

impl SecretCipher {
    fn encrypt(&self, plaintext: &str) -> Result<EncryptedSecret, String> {
        let unbound = UnboundKey::new(&AES_256_GCM, &self.key)
            .map_err(|_| "failed to initialize secret cipher".to_string())?;
        let cipher = LessSafeKey::new(unbound);
        let mut nonce_bytes = [0_u8; 12];
        SystemRandom::new()
            .fill(&mut nonce_bytes)
            .map_err(|_| "failed to generate nonce for secret encryption".to_string())?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut ciphertext = plaintext.as_bytes().to_vec();
        cipher
            .seal_in_place_append_tag(nonce, Aad::empty(), &mut ciphertext)
            .map_err(|_| "failed to encrypt secret value".to_string())?;

        Ok(EncryptedSecret {
            nonce_b64: URL_SAFE_NO_PAD.encode(nonce_bytes),
            ciphertext_b64: URL_SAFE_NO_PAD.encode(&ciphertext),
        })
    }

    fn decrypt(&self, nonce_b64: &str, ciphertext_b64: &str) -> Result<String, String> {
        let nonce_bytes = URL_SAFE_NO_PAD
            .decode(nonce_b64)
            .map_err(|error| format!("failed to decode secret nonce: {error}"))?;
        if nonce_bytes.len() != 12 {
            return Err("invalid secret nonce length".to_string());
        }
        let mut ciphertext = URL_SAFE_NO_PAD
            .decode(ciphertext_b64)
            .map_err(|error| format!("failed to decode secret ciphertext: {error}"))?;

        let unbound = UnboundKey::new(&AES_256_GCM, &self.key)
            .map_err(|_| "failed to initialize secret cipher".to_string())?;
        let cipher = LessSafeKey::new(unbound);
        let nonce = Nonce::assume_unique_for_key([
            nonce_bytes[0],
            nonce_bytes[1],
            nonce_bytes[2],
            nonce_bytes[3],
            nonce_bytes[4],
            nonce_bytes[5],
            nonce_bytes[6],
            nonce_bytes[7],
            nonce_bytes[8],
            nonce_bytes[9],
            nonce_bytes[10],
            nonce_bytes[11],
        ]);
        let plaintext = cipher
            .open_in_place(nonce, Aad::empty(), &mut ciphertext)
            .map_err(|_| "failed to decrypt secret value".to_string())?;

        String::from_utf8(plaintext.to_vec())
            .map_err(|error| format!("secret plaintext was not utf-8: {error}"))
    }
}

pub fn memory_secret_store() -> Arc<dyn SecretStore> {
    Arc::new(InMemorySecretStore::new())
}

pub fn secret_store_from_env(database_url: &str) -> Result<Arc<dyn SecretStore>, String> {
    let requested = std::env::var("GUM_SECRET_BACKEND").ok();
    let master_key = secret_master_key_from_env()?;

    let backend = requested
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| {
            if master_key.is_some() {
                "postgres".to_string()
            } else {
                "memory".to_string()
            }
        });

    match backend.as_str() {
        "memory" => Ok(memory_secret_store()),
        "postgres" | "postgresql" => {
            let key = master_key.ok_or_else(|| {
                "GUM_SECRET_MASTER_KEY is required when GUM_SECRET_BACKEND=postgres".to_string()
            })?;
            let store = PostgresSecretStore::connect(database_url, key)?;
            Ok(Arc::new(store))
        }
        other => Err(format!(
            "unsupported GUM_SECRET_BACKEND '{other}' (expected 'memory' or 'postgres')"
        )),
    }
}

fn secret_master_key_from_env() -> Result<Option<[u8; 32]>, String> {
    match std::env::var("GUM_SECRET_MASTER_KEY") {
        Ok(value) => parse_master_key(&value).map(Some),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err("GUM_SECRET_MASTER_KEY is not valid unicode".to_string())
        }
    }
}

fn parse_master_key(raw: &str) -> Result<[u8; 32], String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err("GUM_SECRET_MASTER_KEY cannot be empty".to_string());
    }

    if let Some(bytes) = decode_hex_32(value) {
        return Ok(bytes);
    }

    for decode in [B64_STANDARD.decode(value), URL_SAFE_NO_PAD.decode(value)] {
        if let Ok(bytes) = decode {
            if bytes.len() == 32 {
                let mut key = [0_u8; 32];
                key.copy_from_slice(&bytes);
                return Ok(key);
            }
        }
    }

    let bytes = value.as_bytes();
    if bytes.len() == 32 {
        let mut key = [0_u8; 32];
        key.copy_from_slice(bytes);
        return Ok(key);
    }

    Err(
        "GUM_SECRET_MASTER_KEY must decode to 32 bytes (hex, base64, or raw 32-byte string)"
            .to_string(),
    )
}

fn decode_hex_32(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }

    let mut out = [0_u8; 32];
    let bytes = value.as_bytes();
    for (index, chunk) in bytes.chunks_exact(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out[index] = (hi << 4) | lo;
    }

    Some(out)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn now_epoch_ms() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as i64,
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_master_key, SecretCipher};
    use base64::engine::general_purpose::STANDARD as B64_STANDARD;
    use base64::Engine as _;

    #[test]
    fn master_key_parser_accepts_hex() {
        let raw = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let key = parse_master_key(raw).expect("hex key should parse");
        assert_eq!(key[0], 0x00);
        assert_eq!(key[1], 0x11);
        assert_eq!(key[31], 0xff);
    }

    #[test]
    fn master_key_parser_accepts_base64() {
        let bytes = [42_u8; 32];
        let raw = B64_STANDARD.encode(bytes);
        let key = parse_master_key(&raw).expect("base64 key should parse");
        assert_eq!(key, bytes);
    }

    #[test]
    fn secret_cipher_roundtrip_works() {
        let cipher = SecretCipher {
            key: [9_u8; 32],
            key_version: 1,
        };
        let encrypted = cipher
            .encrypt("super-secret-value")
            .expect("encryption should succeed");
        let decrypted = cipher
            .decrypt(&encrypted.nonce_b64, &encrypted.ciphertext_b64)
            .expect("decryption should succeed");
        assert_eq!(decrypted, "super-secret-value");
    }
}

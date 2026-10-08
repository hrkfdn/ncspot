//! Resolution of the personal Spotify Web API client ID.
//!
//! The ID can come from the `--client-id` flag, the `NCSPOT_CLIENT_ID` environment variable or
//! the `client_id` configuration value, in that order of precedence. Without one, all Web API
//! traffic uses the shared ncspot client ID ([DEFAULT_WEB_API_CLIENT_ID]).

use std::fmt;

use crate::authentication::DEFAULT_WEB_API_CLIENT_ID;

/// Environment variable that holds the personal Web API client ID.
pub const CLIENT_ID_ENV_VAR: &str = "NCSPOT_CLIENT_ID";

/// Where the personal client ID was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientIdSource {
    Flag,
    Env,
    Config,
}

impl fmt::Display for ClientIdSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Flag => write!(f, "--client-id"),
            Self::Env => write!(f, "{CLIENT_ID_ENV_VAR}"),
            Self::Config => write!(f, "client_id in the configuration file"),
        }
    }
}

/// A personal Web API client ID and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonalClientId {
    pub id: String,
    pub source: ClientIdSource,
}

impl PersonalClientId {
    /// The ID shortened for logs: its first 4 characters followed by `...`.
    pub fn redacted(&self) -> String {
        format!("{}...", &self.id[..4])
    }
}

/// Whether `id` has the format of a Spotify client ID: 32 hexadecimal characters.
fn is_valid_client_id(id: &str) -> bool {
    id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit())
}

/// Resolve the personal Web API client ID from `flag` > `env` > `config`.
///
/// Empty or whitespace-only values count as unset. The first set value must be a valid client
/// ID, otherwise an error naming its source is returned: an invalid value never falls through to
/// a lower source, so a typo is not silently replaced by another ID. A value equal to
/// [DEFAULT_WEB_API_CLIENT_ID] selects the default explicitly and stops resolution.
///
/// `Ok(None)` means there is no personal client ID and all traffic uses the default.
pub fn resolve_personal_client_id(
    flag: Option<&str>,
    env: Option<&str>,
    config: Option<&str>,
) -> Result<Option<PersonalClientId>, String> {
    let sources = [
        (ClientIdSource::Flag, flag),
        (ClientIdSource::Env, env),
        (ClientIdSource::Config, config),
    ];
    for (source, value) in sources {
        let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
            continue;
        };
        if !is_valid_client_id(value) {
            return Err(format!(
                "invalid client ID from {source}: expected 32 hex characters"
            ));
        }
        if value.eq_ignore_ascii_case(DEFAULT_WEB_API_CLIENT_ID) {
            return Ok(None);
        }
        return Ok(Some(PersonalClientId {
            id: value.to_string(),
            source,
        }));
    }
    Ok(None)
}

#[cfg(test)]
mod test {
    use super::*;

    const ID_A: &str = "0123456789abcdef0123456789abcdef";
    const ID_B: &str = "fedcba9876543210fedcba9876543210";
    const ID_C: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn personal(id: &str, source: ClientIdSource) -> Result<Option<PersonalClientId>, String> {
        Ok(Some(PersonalClientId {
            id: id.to_string(),
            source,
        }))
    }

    #[test]
    fn nothing_set_resolves_to_none() {
        assert_eq!(resolve_personal_client_id(None, None, None), Ok(None));
    }

    #[test]
    fn flag_wins_over_env_and_config() {
        assert_eq!(
            resolve_personal_client_id(Some(ID_A), Some(ID_B), Some(ID_C)),
            personal(ID_A, ClientIdSource::Flag)
        );
    }

    #[test]
    fn env_wins_over_config() {
        assert_eq!(
            resolve_personal_client_id(None, Some(ID_B), Some(ID_C)),
            personal(ID_B, ClientIdSource::Env)
        );
    }

    #[test]
    fn config_is_used_when_it_is_the_only_source() {
        assert_eq!(
            resolve_personal_client_id(None, None, Some(ID_C)),
            personal(ID_C, ClientIdSource::Config)
        );
    }

    #[test]
    fn empty_and_whitespace_values_fall_through() {
        assert_eq!(
            resolve_personal_client_id(Some(""), Some("   "), Some(ID_C)),
            personal(ID_C, ClientIdSource::Config)
        );
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        let padded = format!(" {ID_A}\n");
        assert_eq!(
            resolve_personal_client_id(Some(&padded), None, None),
            personal(ID_A, ClientIdSource::Flag)
        );
    }

    #[test]
    fn invalid_higher_source_errors_even_if_lower_source_is_valid() {
        let err = resolve_personal_client_id(None, Some("not-a-client-id"), Some(ID_C))
            .unwrap_err();
        assert!(err.contains("NCSPOT_CLIENT_ID"), "{err}");
        assert!(err.contains("expected 32 hex characters"), "{err}");
    }

    #[test]
    fn wrong_length_is_invalid_and_names_the_flag() {
        let err = resolve_personal_client_id(Some(&ID_A[..31]), None, None).unwrap_err();
        assert!(err.contains("--client-id"), "{err}");
    }

    #[test]
    fn non_hex_is_invalid_and_names_the_config_file() {
        let non_hex = "g".repeat(32);
        let err = resolve_personal_client_id(None, None, Some(&non_hex)).unwrap_err();
        assert!(err.contains("client_id in the configuration file"), "{err}");
    }

    #[test]
    fn default_id_selects_the_default_and_stops_resolution() {
        assert_eq!(
            resolve_personal_client_id(Some(DEFAULT_WEB_API_CLIENT_ID), Some(ID_B), Some(ID_C)),
            Ok(None)
        );
    }

    #[test]
    fn redacted_shows_only_the_first_four_characters() {
        let personal = PersonalClientId {
            id: ID_A.to_string(),
            source: ClientIdSource::Flag,
        };
        assert_eq!(personal.redacted(), "0123...");
    }
}

use std::{env, fmt};

#[derive(Clone)]
pub struct Secret(pub String);
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub api_url: String,
    pub admin_token: Secret,
    pub session_secret: Secret,
    pub client_id: String,
    pub client_secret: Secret,
    pub redirect_uri: String,
    pub bind: String,
    pub dev_user: Option<String>,
    pub discord_api: String,
}
#[must_use]
pub fn development_user(debug: bool, id: Option<String>) -> Option<String> {
    id.filter(|value| debug && crate::session::snowflake(value))
}
impl Config {
    /// # Errors
    /// Returns the name of a missing or invalid required variable.
    pub fn from_env() -> Result<Self, String> {
        let required = |name: &str| {
            env::var(name)
                .ok()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("Missing required environment variable: {name}"))
        };
        let session_secret = required("SESSION_SECRET")?;
        if session_secret.chars().count() < 32 {
            return Err("SESSION_SECRET must be at least 32 characters".into());
        }
        Ok(Self {
            api_url: required("CLOSURE_API_URL")?,
            admin_token: Secret(required("CLOSURE_ADMIN_TOKEN")?),
            session_secret: Secret(session_secret),
            client_id: required("DISCORD_CLIENT_ID")?,
            client_secret: Secret(required("DISCORD_CLIENT_SECRET")?),
            redirect_uri: required("DISCORD_REDIRECT_URI")?,
            bind: env::var("DASHBOARD_BIND").unwrap_or_else(|_| "127.0.0.1:3000".into()),
            dev_user: development_user(
                cfg!(debug_assertions),
                env::var("DASHBOARD_DEV_USER_ID").ok(),
            ),
            discord_api: "https://discord.com/api/v10".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn development_user_is_disabled_in_release() {
        assert!(super::development_user(false, Some("123".into())).is_none());
        assert_eq!(
            super::development_user(true, Some("123".into())).as_deref(),
            Some("123")
        );
        assert!(super::development_user(true, Some("bad".into())).is_none());
    }
}

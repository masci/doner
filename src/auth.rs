use anyhow::{anyhow, Context, Result};
use keyring::Entry;

const SERVICE_NAME: &str = "doner-cli";
const USERNAME: &str = "github-token";

/// Get the keyring entry for the GitHub token
fn get_entry() -> Result<Entry> {
    Entry::new(SERVICE_NAME, USERNAME)
        .map_err(|e| anyhow!("Failed to create keyring entry: {} (kind: {:?})", e, e))
}

/// Store a GitHub token in the system keychain
pub fn store_token(token: &str) -> Result<()> {
    let entry = get_entry()?;
    match entry.set_password(token) {
        Ok(()) => Ok(()),
        Err(e) => Err(anyhow!(
            "Failed to store token in keychain: {} (debug: {:?})",
            e,
            e
        )),
    }
}

/// Retrieve the stored GitHub token from the system keychain
pub fn get_token() -> Result<String> {
    let entry = get_entry()?;
    entry
        .get_password()
        .map_err(|e| anyhow!("Failed to retrieve token from keychain: {}", e))
}

/// Delete the stored GitHub token from the system keychain
pub fn delete_token() -> Result<()> {
    let entry = get_entry()?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // Already deleted, that's fine
        Err(e) => Err(anyhow!("Failed to delete token from keychain: {}", e)),
    }
}

/// Check if a token is stored
pub fn has_token() -> bool {
    get_token().is_ok()
}

/// Get a token from environment variable or keychain
/// Priority: GITHUB_TOKEN env var > stored token
pub fn resolve_token() -> Result<String> {
    // First try environment variable
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        return Ok(token);
    }

    // Then try keychain
    get_token().map_err(|_| {
        anyhow!(
            "No GitHub token found. Either:\n  \
             1. Run 'doner auth login' to authenticate\n  \
             2. Set the GITHUB_TOKEN environment variable"
        )
    })
}

/// Interactive login - prompts for token
pub fn interactive_login() -> Result<String> {
    println!("Paste your GitHub personal access token:");
    println!("(Create one at https://github.com/settings/tokens with 'read:project' and 'repo' scopes)");
    println!();

    let token = rpassword::read_password().context("Failed to read token")?;

    let token = token.trim().to_string();

    if token.is_empty() {
        return Err(anyhow!("Token cannot be empty"));
    }

    Ok(token)
}

/// Validate a token by making a test API call
pub async fn validate_token(token: &str) -> Result<String> {
    let client = reqwest::Client::new();

    let response = client
        .post("https://api.github.com/graphql")
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "doner-cli")
        .json(&serde_json::json!({
            "query": "query { viewer { login } }"
        }))
        .send()
        .await
        .context("Failed to connect to GitHub API")?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Invalid token or authentication failed (HTTP {})",
            response.status()
        ));
    }

    #[derive(serde::Deserialize)]
    struct Response {
        data: Option<Data>,
    }

    #[derive(serde::Deserialize)]
    struct Data {
        viewer: Viewer,
    }

    #[derive(serde::Deserialize)]
    struct Viewer {
        login: String,
    }

    let body: Response = response.json().await.context("Failed to parse response")?;

    let username = body
        .data
        .map(|d| d.viewer.login)
        .ok_or_else(|| anyhow!("Failed to get user info"))?;

    Ok(username)
}

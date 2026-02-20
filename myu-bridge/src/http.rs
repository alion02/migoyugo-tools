use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};

use crate::models::{AcceptChallengeResponse, LoginRequest, LoginResponse};

pub struct MigoyugoHttpClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl MigoyugoHttpClient {
    pub fn new(base_url: &str) -> Self {
        Self { client: Client::new(), base_url: base_url.trim_end_matches('/').to_string(), token: None }
    }

    pub fn set_token(&mut self, token: String) {
        self.token = Some(token);
    }

    pub fn get_token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub async fn login(&mut self, email: &str, password: &str) -> Result<()> {
        let url = format!("{}/api/auth/login", self.base_url);
        let req_body = LoginRequest { email, password };

        tracing::debug!("Sending POST request to {}", url);
        let res = self.client.post(&url).json(&req_body).send().await.context("Failed to send login request")?;

        if !res.status().is_success() {
            let status = res.status();
            let headers = res.headers().clone();
            let text = res.text().await.unwrap_or_default();

            tracing::error!(
                "Login request failed.\nURL: {}\nStatus: {}\nHeaders: {:#?}\nResponse Body: {}",
                url,
                status,
                headers,
                text
            );

            anyhow::bail!("Login failed with status {}: {}", status, text);
        }

        let data: LoginResponse = res.json().await.context("Failed to parse login response")?;
        self.set_token(data.token);

        tracing::info!("Successfully logged in to migoyugo.com");
        Ok(())
    }

    fn auth_header(&self) -> Result<String> {
        let token = self.token.as_ref().context("Not logged in (no token)")?;
        Ok(format!("Bearer {}", token))
    }

    pub async fn accept_challenge(&self, challenge_id: u64) -> Result<String> {
        let url = format!("{}/api/auth/challenges/{}/accept", self.base_url, challenge_id);

        let res = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("Failed to send accept challenge request")?;

        if res.status() != StatusCode::OK {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            anyhow::bail!("Failed to accept challenge ({}): {}", status, text);
        }

        let data: AcceptChallengeResponse = res.json().await.context("Failed to parse accept challenge response")?;

        Ok(data.game_id)
    }

    pub async fn decline_challenge(&self, challenge_id: u64) -> Result<()> {
        let url = format!("{}/api/auth/challenges/{}/decline", self.base_url, challenge_id);

        let res = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("Failed to send decline challenge request")?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            anyhow::bail!("Failed to decline challenge ({}): {}", status, text);
        }

        Ok(())
    }
}

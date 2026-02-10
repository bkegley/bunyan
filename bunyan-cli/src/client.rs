use reqwest::blocking::Client;
use serde::de::DeserializeOwned;

pub struct BunyanClient {
    base_url: String,
    client: Client,
}

impl BunyanClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: Client::new(),
        }
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .send()
            .map_err(|e| format!("Request failed: {}", e))?;
        handle_response(resp)
    }

    pub fn post<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .json(body)
            .send()
            .map_err(|e| format!("Request failed: {}", e))?;
        handle_response(resp)
    }

    pub fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .send()
            .map_err(|e| format!("Request failed: {}", e))?;
        handle_response(resp)
    }

    pub fn put<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .put(&url)
            .json(body)
            .send()
            .map_err(|e| format!("Request failed: {}", e))?;
        handle_response(resp)
    }

    pub fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .delete(&url)
            .send()
            .map_err(|e| format!("Request failed: {}", e))?;
        handle_response(resp)
    }
}

/// Visible for testing — extract the base URL.
#[cfg(test)]
impl BunyanClient {
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

fn handle_response<T: DeserializeOwned>(resp: reqwest::blocking::Response) -> Result<T, String> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        if let Ok(err) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(msg) = err.get("error").and_then(|e| e.as_str()) {
                return Err(msg.to_string());
            }
        }
        return Err(format!("HTTP {}: {}", status, body));
    }
    resp.json::<T>().map_err(|e| format!("Failed to parse response: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_strips_trailing_slash() {
        let client = BunyanClient::new("http://localhost:3333/");
        assert_eq!(client.base_url(), "http://localhost:3333");
    }

    #[test]
    fn new_preserves_url_without_trailing_slash() {
        let client = BunyanClient::new("http://localhost:3333");
        assert_eq!(client.base_url(), "http://localhost:3333");
    }

    #[test]
    fn new_strips_multiple_trailing_slashes() {
        let client = BunyanClient::new("http://localhost:3333///");
        assert_eq!(client.base_url(), "http://localhost:3333");
    }
}

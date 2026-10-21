//! Downloads from the official sites, with limits: a browser-like user agent (some of them refuse others), a timeout,
//! a cap on the size, and byte ranges for large ZIP archives (only their directory and the tables wanted come down;
//! the whole archive when the server does not serve ranges).

use std::time::Duration;

use reqwest::header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, RANGE, USER_AGENT};
use reqwest::StatusCode;

use crate::zip::{self, Entry};

const AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Coucou/0.4 (Zheus)";
/// The most a single download may be.
pub const MAX_BYTES: u64 = 400 * 1024 * 1024;

#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
}

fn short(url: &str) -> String {
    url.split('?').next().unwrap_or(url).rsplit('/').take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("/")
}

impl Http {
    pub fn new() -> Http {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Http { client }
    }

    /// The whole body of `url`, up to `MAX_BYTES`.
    pub async fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        let response = self
            .client
            .get(url)
            .header(USER_AGENT, AGENT)
            .send()
            .await
            .map_err(|e| format!("{}: {}", short(url), if e.is_timeout() { "no answer in time".to_string() } else { "could not connect".to_string() }))?;
        if !response.status().is_success() {
            return Err(format!("{} answered {}", short(url), response.status().as_u16()));
        }
        read_capped(response, url).await
    }

    pub async fn text(&self, url: &str) -> Result<String, String> {
        self.get(url).await.map(|b| crate::text::decode(&b))
    }

    /// The bytes `from..to` (end excluded) of `url`. Err when the server sent something else than that range.
    async fn range(&self, url: &str, from: u64, to: u64) -> Result<Vec<u8>, String> {
        let response = self
            .client
            .get(url)
            .header(USER_AGENT, AGENT)
            .header(RANGE, format!("bytes={from}-{}", to.saturating_sub(1)))
            .send()
            .await
            .map_err(|_| format!("{}: could not connect", short(url)))?;
        if response.status() != StatusCode::PARTIAL_CONTENT {
            return Err(format!("{} does not serve ranges ({})", short(url), response.status().as_u16()));
        }
        read_capped(response, url).await
    }

    /// The size of `url` and whether it serves ranges, from a one-byte range request (some servers answer HEAD badly).
    async fn probe(&self, url: &str) -> Result<(u64, bool), String> {
        let response = self
            .client
            .get(url)
            .header(USER_AGENT, AGENT)
            .header(RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(|_| format!("{}: could not connect", short(url)))?;
        let status = response.status();
        if status == StatusCode::PARTIAL_CONTENT {
            let total = response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.rsplit('/').next())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .ok_or_else(|| format!("{} gave no size", short(url)))?;
            return Ok((total, true));
        }
        if !status.is_success() {
            return Err(format!("{} answered {}", short(url), status.as_u16()));
        }
        let ranges = response.headers().get(ACCEPT_RANGES).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains("bytes"));
        let total = response.headers().get(CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0);
        Ok((total, ranges && total > 0))
    }

    /// The entries of a ZIP archive at `url` whose names end with one of `wanted` (case-insensitive), as (name,
    /// bytes): by byte ranges when the server serves them, else from the whole archive.
    pub async fn zip_entries(&self, url: &str, wanted: &[String]) -> Result<Vec<(String, Vec<u8>)>, String> {
        let pick = |entries: &[Entry]| -> Vec<Entry> {
            entries
                .iter()
                .filter(|e| wanted.iter().any(|w| e.name.to_ascii_lowercase().ends_with(&w.to_ascii_lowercase())))
                .cloned()
                .collect()
        };
        if let Ok((total, true)) = self.probe(url).await {
            let tail_from = total.saturating_sub(zip::TAIL);
            let tail = self.range(url, tail_from, total).await?;
            let (offset, size) = zip::directory_location(&tail, total)?;
            let directory = if offset >= tail_from {
                tail[(offset - tail_from) as usize..(offset - tail_from + size) as usize].to_vec()
            } else {
                self.range(url, offset, offset + size).await?
            };
            let mut out = Vec::new();
            for entry in pick(&zip::entries(&directory)?) {
                if entry.compressed > MAX_BYTES {
                    return Err(format!("{} is too large", entry.name));
                }
                let local = self.range(url, entry.offset, (entry.offset + zip::span(&entry)).min(total)).await?;
                out.push((entry.name.clone(), zip::extract(&entry, &local)?));
            }
            return Ok(out);
        }
        let bytes = self.get(url).await?;
        let archive = zip::Archive::new(&bytes)?;
        let mut out = Vec::new();
        for entry in pick(&archive.entries) {
            out.push((entry.name.clone(), archive.read(&entry)?));
        }
        Ok(out)
    }
}

impl Default for Http {
    fn default() -> Self {
        Http::new()
    }
}

async fn read_capped(mut response: reqwest::Response, url: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| format!("{}: the download stopped", short(url)))? {
        out.extend_from_slice(&chunk);
        if out.len() as u64 > MAX_BYTES {
            return Err(format!("{} is too large", short(url)));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_name_the_file_not_the_query() {
        assert_eq!(short("https://dados.cvm.gov.br/dados/FII/DOC/INF_MENSAL/DADOS/inf_mensal_fii_2026.zip"), "DADOS/inf_mensal_fii_2026.zip");
        assert_eq!(short("https://api.bcb.gov.br/dados/serie/bcdata.sgs.432/dados/ultimos/1?formato=json"), "ultimos/1");
    }
}

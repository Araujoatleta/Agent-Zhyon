// The user's Google Sheet through AXYON Core's own Google sign-in, when gog is not on the computer: the token its
// google-workspace skill keeps in Hermes' home (google_token.json, made by that skill's setup when the user signed in
// to Google there). AXYON reads that file when it needs it, gets a fresh access token from Google with it when the
// one there is old (kept in memory only: nothing is ever written back), and calls the Sheets API with it, for the
// spreadsheet the user linked and nothing else: its tabs' names, a range's values, a range written, a tab added.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::Url;
use serde_json::{json, Value};

use crate::i18n::{t, tf};

const SHEETS: &str = "https://sheets.googleapis.com/v4/spreadsheets/";
/// Google's own token endpoint: a refresh goes nowhere else, whatever the file says.
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const WAIT: Duration = Duration::from_secs(30);
/// The largest answer read from Google.
const MAX_ANSWER: usize = 8 * 1024 * 1024;

/// An access token, and until when it holds (unix seconds).
#[derive(Debug, Clone, PartialEq)]
struct Access {
    token: String,
    until: u64,
}

/// The access token got last, in memory.
static ACCESS: Mutex<Option<Access>> = Mutex::new(None);

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Where AXYON Core's google-workspace skill keeps its sign-in: Hermes' home (the one AXYON runs, or HERMES_HOME), else
/// the usual ~/.hermes.
pub fn token_file() -> Option<PathBuf> {
    let homes = [crate::hermes_setup::hermes_home(), crate::platform::home_dir().join(".hermes")];
    homes.iter().map(|h| h.join("google_token.json")).find(|p| p.is_file())
}

/// "2026-10-27T21:00:00.123456Z" (google-auth's `expiry`) in unix seconds.
fn expiry_secs(text: &str) -> Option<u64> {
    let text = text.trim();
    let date = coucou_powers::date::Date::parse(text.get(..10)?)?;
    let clock = text.get(11..19)?;
    let mut parts = clock.split(':').map(|p| p.parse::<i64>().ok());
    let (h, m, s) = (parts.next()??, parts.next()??, parts.next()??);
    u64::try_from(date.days() * 86_400 + h * 3600 + m * 60 + s).ok()
}

/// The access token in the file, while it holds for another minute.
fn stored_access(file: &Value, at: u64) -> Option<Access> {
    let token = file["token"].as_str().filter(|t| !t.is_empty())?;
    let until = expiry_secs(file["expiry"].as_str()?)?;
    (until > at + 60).then(|| Access { token: token.to_string(), until })
}

/// The form a refresh sends Google.
fn refresh_form(file: &Value) -> Result<String, String> {
    let field = |k: &str| file[k].as_str().filter(|v| !v.is_empty()).map(str::to_string);
    let (Some(id), Some(secret), Some(refresh)) = (field("client_id"), field("client_secret"), field("refresh_token")) else {
        return Err(t("AXYON Core's Google sign-in cannot be renewed: ask it to sign in to Google again (its google-workspace skill)."));
    };
    let mut form = Url::parse("https://form.invalid/").map_err(|e| e.to_string())?;
    form.query_pairs_mut().append_pair("client_id", &id).append_pair("client_secret", &secret).append_pair("refresh_token", &refresh).append_pair("grant_type", "refresh_token");
    Ok(form.query().unwrap_or_default().to_string())
}

fn client() -> Result<reqwest::Client, String> {
    let url = Url::parse(SHEETS).map_err(|e| e.to_string())?;
    crate::net::client(&url, WAIT)
}

/// Google's own words about a failure: `error.message` (or `error_description`), cut down.
fn google_words(body: &[u8]) -> String {
    let json: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let said = json["error"]["message"].as_str().or_else(|| json["error_description"].as_str()).or_else(|| json["error"].as_str()).unwrap_or("");
    coucou_powers::text::clip(said, 300)
}

/// An access token: the one got last while it holds, else the file's, else a new one from Google.
async fn access(fresh: bool) -> Result<String, String> {
    let at = now();
    if !fresh {
        if let Some(a) = ACCESS.lock().unwrap_or_else(|e| e.into_inner()).clone().filter(|a| a.until > at + 60) {
            return Ok(a.token);
        }
    }
    let path = token_file().ok_or_else(|| t("AXYON Core is not signed in to Google on this computer: ask it to set up its google-workspace skill (it asks for your Google sign-in once)."))?;
    let file: Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|_| t("AXYON Core's Google sign-in file could not be read."))?;
    if !fresh {
        if let Some(a) = stored_access(&file, at) {
            *ACCESS.lock().unwrap_or_else(|e| e.into_inner()) = Some(a.clone());
            return Ok(a.token);
        }
    }
    let response = client()?
        .post(TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(refresh_form(&file)?)
        .send()
        .await
        .map_err(|e| tf("Google could not be reached: {error}", &[("error", &e.to_string())]))?;
    let status = response.status();
    let body = crate::net::read_capped(response, MAX_ANSWER).await?;
    if !status.is_success() {
        return Err(tf("AXYON Core's Google sign-in no longer works ({error}): ask it to sign in to Google again (its google-workspace skill).", &[("error", &google_words(&body))]));
    }
    let json: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let token = json["access_token"].as_str().filter(|t| !t.is_empty()).ok_or_else(|| t("Google gave no access token."))?;
    let a = Access { token: token.to_string(), until: at + json["expires_in"].as_u64().unwrap_or(3600) };
    *ACCESS.lock().unwrap_or_else(|e| e.into_inner()) = Some(a.clone());
    Ok(a.token)
}

/// What Google answered: the JSON of a success, else Google's words (a range in a tab that is not there says "Unable to
/// parse range", as gog relays it).
async fn call(method: reqwest::Method, url: Url, body: Option<Value>) -> Result<Value, String> {
    for attempt in 0..2 {
        let token = access(attempt > 0).await?;
        let mut request = client()?.request(method.clone(), url.clone()).bearer_auth(token);
        if let Some(b) = &body {
            request = request.json(b);
        }
        let response = request.send().await.map_err(|e| tf("Google could not be reached: {error}", &[("error", &e.to_string())]))?;
        let status = response.status();
        let answer = crate::net::read_capped(response, MAX_ANSWER).await?;
        // An access token Google no longer takes: a new one, once.
        if status.as_u16() == 401 && attempt == 0 {
            continue;
        }
        if status.is_success() {
            return Ok(serde_json::from_slice(&answer).unwrap_or(Value::Null));
        }
        let words = google_words(&answer);
        return Err(match status.as_u16() {
            401 => tf("AXYON Core's Google sign-in no longer works ({error}): ask it to sign in to Google again (its google-workspace skill).", &[("error", &words)]),
            403 => tf("Google refused AXYON Core's sign-in for this spreadsheet: {error}", &[("error", &words)]),
            404 => t("Google has no spreadsheet with this link, or this Google account cannot open it."),
            _ => words,
        });
    }
    Err(t("Google refused the sign-in."))
}

/// The address of `parts` under the spreadsheet (each part encoded as one piece of the path).
fn url_of(id: &str, parts: &[&str], query: &[(&str, &str)]) -> Result<Url, String> {
    let mut url = Url::parse(SHEETS).map_err(|e| e.to_string())?;
    {
        let mut path = url.path_segments_mut().map_err(|_| "bad address".to_string())?;
        path.pop_if_empty().push(id);
        for p in parts {
            path.push(p);
        }
    }
    for (k, v) in query {
        url.query_pairs_mut().append_pair(k, v);
    }
    Ok(url)
}

/// `range`'s values, as numbers where they are numbers: `{"range", "values"}`, as gog's `sheets get --json` gives it.
pub async fn values(id: &str, range: &str) -> Result<Value, String> {
    call(reqwest::Method::GET, url_of(id, &["values", range], &[("valueRenderOption", "UNFORMATTED_VALUE")])?, None).await
}

/// Writes `values` into `range`, raw (a text is never taken for a formula).
pub async fn update(id: &str, range: &str, values: &[Vec<Value>]) -> Result<(), String> {
    let body = json!({ "range": range, "majorDimension": "ROWS", "values": values });
    call(reqwest::Method::PUT, url_of(id, &["values", range], &[("valueInputOption", "RAW")])?, Some(body)).await.map(|_| ())
}

/// Adds the tab `title`.
pub async fn add_tab(id: &str, title: &str) -> Result<(), String> {
    let body = json!({ "requests": [{ "addSheet": { "properties": { "title": title } } }] });
    let mut url = url_of(id, &[], &[])?;
    // POST …/spreadsheets/{id}:batchUpdate.
    let path = format!("{}:batchUpdate", url.path());
    url.set_path(&path);
    call(reqwest::Method::POST, url, Some(body)).await.map(|_| ())
}

/// The names of the spreadsheet's tabs, in order.
pub async fn tabs(id: &str) -> Result<Vec<String>, String> {
    let answer = call(reqwest::Method::GET, url_of(id, &[], &[("fields", "sheets.properties.title")])?, None).await?;
    Ok(titles_of(&answer))
}

/// The tabs' names in a spreadsheet's description (the Sheets API's, or gog's `sheets metadata --json`, which may wrap
/// it): every `sheets[].properties.title`.
pub fn titles_of(answer: &Value) -> Vec<String> {
    let sheets = answer.get("sheets").or_else(|| answer.get("spreadsheet").and_then(|s| s.get("sheets"))).and_then(Value::as_array);
    sheets
        .map(|list| list.iter().filter_map(|s| s["properties"]["title"].as_str().or_else(|| s["title"].as_str()).map(str::to_string)).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sign_in_file_says_its_token_and_how_to_renew_it() {
        assert_eq!(expiry_secs("2026-10-27T21:00:00.123456Z"), Some(1_793_134_800));
        assert_eq!(expiry_secs("2026-10-27T21:00:00Z"), Some(1_793_134_800));
        assert_eq!(expiry_secs("soon"), None);
        let file = json!({"token": "ya29.abc", "expiry": "2026-10-27T21:00:00Z", "refresh_token": "1//r", "client_id": "id.apps", "client_secret": "s/x", "token_uri": "https://evil.example/token"});
        assert_eq!(stored_access(&file, 1_793_134_800 - 3600), Some(Access { token: "ya29.abc".into(), until: 1_793_134_800 }));
        assert_eq!(stored_access(&file, 1_793_134_800 - 30), None, "a token about to end is renewed");
        let form = refresh_form(&file).unwrap();
        assert_eq!(form, "client_id=id.apps&client_secret=s%2Fx&refresh_token=1%2F%2Fr&grant_type=refresh_token");
        assert!(!form.contains("evil"), "the file's token_uri is not where the secret goes");
        assert!(refresh_form(&json!({"token": "x"})).is_err());
    }

    #[test]
    fn the_spreadsheet_is_addressed_piece_by_piece() {
        let id = "1AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-xyz";
        let url = url_of(id, &["values", "'Ana''s'!A1:Z400"], &[("valueRenderOption", "UNFORMATTED_VALUE")]).unwrap();
        assert_eq!(url.as_str(), format!("https://sheets.googleapis.com/v4/spreadsheets/{id}/values/'Ana''s'!A1:Z400?valueRenderOption=UNFORMATTED_VALUE"));
        let url = url_of(id, &["values", "AXYON!A1/B"], &[]).unwrap();
        assert!(url.as_str().ends_with("/values/AXYON!A1%2FB"), "a slash stays inside its piece: {url}");
        let answer = json!({"sheets": [{"properties": {"title": "Meu Patrimônio"}}, {"properties": {"title": "Preços Teto"}}]});
        assert_eq!(titles_of(&answer), ["Meu Patrimônio", "Preços Teto"]);
        assert_eq!(titles_of(&json!({"spreadsheet": {"sheets": [{"title": "A"}]}})), ["A"]);
        assert!(titles_of(&json!({})).is_empty());
        assert_eq!(google_words(br#"{"error": {"code": 400, "message": "Unable to parse range: 'AXYON'!A1:Z400"}}"#), "Unable to parse range: 'AXYON'!A1:Z400");
        assert_eq!(google_words(br#"{"error": "invalid_grant", "error_description": "Token has been expired or revoked."}"#), "Token has been expired or revoked.");
    }
}

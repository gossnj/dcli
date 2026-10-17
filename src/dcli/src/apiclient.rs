/*
* Copyright 2023 Mike Chambers
* https://github.com/mikechambers/dcli
*
* Permission is hereby granted, free of charge, to any person obtaining a copy of
* this software and associated documentation files (the "Software"), to deal in
* the Software without restriction, including without limitation the rights to
* use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
* of the Software, and to permit persons to whom the Software is furnished to do
* so, subject to the following conditions:
*
* The above copyright notice and this permission notice shall be included in all
* copies or substantial portions of the Software.
*
* THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
* IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
* FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
* COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
* IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
* CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
*/

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use async_compression::tokio::bufread::GzipDecoder;
use chrono::{DateTime, NaiveDateTime, Utc};
use log::info;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT_ENCODING, CONNECTION, CONTENT_ENCODING,
    RETRY_AFTER,
};
use reqwest::{Client, Method, Url};
use tokio::io::AsyncReadExt;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

use crate::error::Error;
use crate::response::drs::{
    check_destiny_response_status, is_destiny_throttle_code,
    IsDestinyAPIResponse,
};

const DESTINY_API_KEY: &str = env!("DESTINY_API_KEY");
const API_TIMEOUT: u64 = 10;
const MAX_API_BODY: usize = 8 * 1024 * 1024;
const MAX_TRANSFER: usize = 16 * 1024 * 1024;
const MAX_ATTEMPTS: usize = 3;
const START_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 100);

static_assertions::const_assert!(!DESTINY_API_KEY.is_empty());

struct RequestBudget {
    in_flight: Arc<Semaphore>,
    body_slots: Arc<Semaphore>,
    timing: Mutex<RequestTiming>,
}

struct RequestTiming {
    next_start: Instant,
    cooldown_until: Instant,
}

impl RequestBudget {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            in_flight: Arc::new(Semaphore::new(25)),
            body_slots: Arc::new(Semaphore::new(4)),
            timing: Mutex::new(RequestTiming {
                next_start: now,
                cooldown_until: now,
            }),
        }
    }

    async fn enter(&self) -> OwnedSemaphorePermit {
        let permit = self
            .in_flight
            .clone()
            .acquire_owned()
            .await
            .expect("request budget is never closed");
        loop {
            let mut timing = self.timing.lock().await;
            let now = Instant::now();
            let ready = timing.next_start.max(timing.cooldown_until);
            if now >= ready {
                timing.next_start = now + START_INTERVAL;
                return permit;
            }
            drop(timing);
            tokio::time::sleep_until(tokio::time::Instant::from_std(ready))
                .await;
        }
    }

    async fn reserve_body(&self) -> OwnedSemaphorePermit {
        self.body_slots
            .clone()
            .acquire_owned()
            .await
            .expect("body budget is never closed")
    }

    async fn cool_down(&self, delay: Duration) {
        let mut timing = self.timing.lock().await;
        if let Some(until) = Instant::now().checked_add(delay) {
            timing.cooldown_until = timing.cooldown_until.max(until);
        }
    }
}

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    budget: Arc<RequestBudget>,
    #[cfg(test)]
    pub(crate) test_endpoint: Option<String>,
}

impl ApiClient {
    pub fn new() -> Result<ApiClient, Error> {
        ApiClient::new_with_key(DESTINY_API_KEY)
    }

    pub fn new_with_key(key: &str) -> Result<ApiClient, Error> {
        let mut headers = HeaderMap::new();
        headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
        headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip"));
        headers.insert(
            "Keep-Alive",
            HeaderValue::from_static("timeout=10, max=1000"),
        );
        let key =
            HeaderValue::from_str(key).map_err(|_| Error::InvalidArgument {
                description: "Invalid API key header".to_string(),
            })?;
        headers.insert("X-API-Key", key);
        let client = Client::builder()
            .default_headers(headers)
            .gzip(false)
            .timeout(Duration::from_secs(API_TIMEOUT))
            .build()?;
        Ok(ApiClient {
            client,
            budget: Arc::new(RequestBudget::new()),
            #[cfg(test)]
            test_endpoint: None,
        })
    }

    fn endpoint(&self, url: &str) -> Result<Url, Error> {
        let url = Url::parse(url).map_err(|_| Error::InvalidArgument {
            description: "Invalid API URL".to_string(),
        })?;
        #[cfg(test)]
        if let Some(base) = &self.test_endpoint {
            let mut redirected =
                Url::parse(base).map_err(|_| Error::InvalidArgument {
                    description: "Invalid test API URL".to_string(),
                })?;
            redirected.set_path(url.path());
            redirected.set_query(url.query());
            return Ok(redirected);
        }
        Ok(url)
    }

    async fn send(
        &self,
        method: Method,
        url: &str,
        body: Option<&str>,
    ) -> Result<(reqwest::Response, OwnedSemaphorePermit), Error> {
        let url = self.endpoint(url)?;
        info!("API call : {}", url);
        tell::verbose!("{}", url);
        let permit = self.budget.enter().await;
        let mut request = self.client.request(method, url);
        if let Some(body) = body {
            request = request.body(body.to_string());
        }
        let response = request.send().await?;
        Ok((response, permit))
    }

    pub async fn call(&self, url: &str) -> Result<reqwest::Response, Error> {
        let (response, _permit) = self.send(Method::GET, url, None).await?;
        Ok(response)
    }

    pub async fn call_post(
        &self,
        url: &str,
        post_data: &str,
    ) -> Result<reqwest::Response, Error> {
        let (response, _permit) =
            self.send(Method::POST, url, Some(post_data)).await?;
        Ok(response)
    }

    pub async fn call_and_parse<
        T: serde::de::DeserializeOwned + IsDestinyAPIResponse,
    >(
        &self,
        url: &str,
    ) -> Result<T, Error> {
        self.call_and_parse_bounded(url)
            .await
            .map(|(value, _permit)| value)
    }

    pub async fn call_and_parse_bounded<
        T: serde::de::DeserializeOwned + IsDestinyAPIResponse,
    >(
        &self,
        url: &str,
    ) -> Result<(T, OwnedSemaphorePermit), Error> {
        self.request_and_parse(Method::GET, url, None).await
    }

    pub async fn call_post_and_parse<
        T: serde::de::DeserializeOwned + IsDestinyAPIResponse,
    >(
        &self,
        url: &str,
        post_data: &str,
    ) -> Result<T, Error> {
        self.request_and_parse(Method::POST, url, Some(post_data))
            .await
            .map(|(value, _permit)| value)
    }

    async fn request_and_parse<
        T: serde::de::DeserializeOwned + IsDestinyAPIResponse,
    >(
        &self,
        method: Method,
        url: &str,
        body: Option<&str>,
    ) -> Result<(T, OwnedSemaphorePermit), Error> {
        for attempt in 0..MAX_ATTEMPTS {
            let result =
                self.single_request::<T>(method.clone(), url, body).await;
            match result {
                Ok(value) => return Ok(value),
                Err(error)
                    if error.is_retryable() && attempt + 1 < MAX_ATTEMPTS =>
                {
                    let jitter = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .subsec_millis()
                        as u64
                        % 50;
                    tokio::time::sleep(Duration::from_millis(
                        100 * (1 << attempt) + jitter,
                    ))
                    .await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!()
    }

    async fn single_request<
        T: serde::de::DeserializeOwned + IsDestinyAPIResponse,
    >(
        &self,
        method: Method,
        url: &str,
        body: Option<&str>,
    ) -> Result<(T, OwnedSemaphorePermit), Error> {
        let (mut response, _permit) = self.send(method, url, body).await?;
        let status = response.status();
        if status.as_u16() == 429 || status.as_u16() == 503 {
            let advertised = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(retry_after_delay);
            let delay =
                advertised.unwrap_or_default().max(Duration::from_secs(1));
            self.budget.cool_down(delay).await;
        }
        if !status.is_success() {
            return Err(Error::ApiHttpStatus {
                status: status.as_u16(),
            });
        }
        let encoding = match response.headers().get(CONTENT_ENCODING) {
            None => false,
            Some(value)
                if value.to_str().is_ok_and(|text| {
                    text.trim().eq_ignore_ascii_case("identity")
                }) =>
            {
                false
            }
            Some(value)
                if value.to_str().is_ok_and(|text| {
                    text.trim().eq_ignore_ascii_case("gzip")
                }) =>
            {
                true
            }
            Some(_) => return Err(Error::ApiUnsupportedEncoding),
        };
        if response
            .content_length()
            .is_some_and(|length| length > MAX_TRANSFER as u64)
        {
            return Err(Error::ApiBodyTooLarge);
        }
        let body_permit = self.budget.reserve_body().await;
        let mut encoded = Vec::new();
        let mut received = 0usize;
        let mut oversized = false;
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            if error.is_timeout() {
                Error::RequestTimedOut
            } else {
                Error::ApiTransport
            }
        })? {
            received = received
                .checked_add(chunk.len())
                .ok_or(Error::ApiBodyTooLarge)?;
            if received > MAX_TRANSFER {
                return Err(Error::ApiBodyTooLarge);
            }
            if received > MAX_API_BODY {
                oversized = true;
            }
            if !oversized {
                encoded.extend_from_slice(&chunk);
            }
        }
        if oversized {
            return Err(Error::ApiBodyTooLarge);
        }
        let bytes = if encoding {
            let mut decoder = GzipDecoder::new(encoded.as_slice());
            let mut decoded = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                let count = decoder
                    .read(&mut chunk)
                    .await
                    .map_err(|_| Error::ApiMalformedEncoding)?;
                if count == 0 {
                    break;
                }
                if count > MAX_API_BODY - decoded.len() {
                    return Err(Error::ApiBodyTooLarge);
                }
                decoded.extend_from_slice(&chunk[..count]);
            }
            decoded
        } else {
            encoded
        };
        let value: T = serde_json::from_slice(&bytes)?;
        let destiny_status = value.get_status();
        if is_destiny_throttle_code(destiny_status.error_code) {
            let delay = Duration::from_secs(
                destiny_status.throttle_seconds.max(1) as u64,
            );
            self.budget.cool_down(delay).await;
        } else if destiny_status.throttle_seconds > 0 {
            self.budget
                .cool_down(Duration::from_secs(
                    destiny_status.throttle_seconds as u64,
                ))
                .await;
        }
        check_destiny_response_status(destiny_status)?;
        Ok((value, body_permit))
    }
}

fn retry_after_delay(value: &HeaderValue) -> Option<Duration> {
    let text = value.to_str().ok()?;
    if let Ok(seconds) = text.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    if let Ok(date) = DateTime::parse_from_rfc2822(text) {
        return Some(
            date.signed_duration_since(Utc::now())
                .to_std()
                .unwrap_or_default(),
        );
    }
    let date = NaiveDateTime::parse_from_str(text, "%a, %d %b %Y %H:%M:%S GMT")
        .ok()?;
    Some(
        date.and_utc()
            .signed_duration_since(Utc::now())
            .to_std()
            .unwrap_or_default(),
    )
}

use std::time::Duration;

use crate::error::AuthError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Body>,
}

#[derive(Debug, Clone)]
pub struct Body {
    pub content_type: &'static str,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, AuthError> {
        if !self.is_success() {
            return Err(AuthError::Status {
                status: self.status,
                url: url.into(),
            });
        }
        Ok(serde_json::from_str(&self.body)?)
    }
}

pub trait HttpClient {
    fn send(&self, request: &Request) -> Result<Response, AuthError>;
}

impl Request {
    pub fn get(url: &str) -> Self {
        Self {
            method: Method::Get,
            url: url.into(),
            headers: vec![("Accept".into(), "application/json".into())],
            body: None,
        }
    }

    pub fn post_json(url: &str, json: &serde_json::Value) -> Self {
        Self {
            method: Method::Post,
            url: url.into(),
            headers: vec![("Accept".into(), "application/json".into())],
            body: Some(Body {
                content_type: "application/json",
                text: json.to_string(),
            }),
        }
    }

    pub fn post_form(url: &str, fields: &[(&str, &str)]) -> Self {
        let text = fields
            .iter()
            .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        Self {
            method: Method::Post,
            url: url.into(),
            headers: Vec::new(),
            body: Some(Body {
                content_type: "application/x-www-form-urlencoded",
                text,
            }),
        }
    }

    pub fn bearer(mut self, token: &str) -> Self {
        self.headers
            .push(("Authorization".into(), format!("Bearer {token}")));
        self
    }
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[derive(Debug)]
pub struct UreqClient {
    agent: ureq::Agent,
}

impl UreqClient {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        Self { agent }
    }
}

impl Default for UreqClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient for UreqClient {
    fn send(&self, request: &Request) -> Result<Response, AuthError> {
        let transport = |e: ureq::Error| AuthError::Transport(e.to_string());
        let mut response = match request.method {
            Method::Get => {
                let mut r = self.agent.get(&request.url);
                for (k, v) in &request.headers {
                    r = r.header(k, v);
                }
                r.call().map_err(transport)?
            }
            Method::Post => {
                let mut r = self.agent.post(&request.url);
                for (k, v) in &request.headers {
                    r = r.header(k, v);
                }
                match &request.body {
                    Some(b) => r
                        .content_type(b.content_type)
                        .send(b.text.as_bytes())
                        .map_err(transport)?,
                    None => r.send_empty().map_err(transport)?,
                }
            }
        };
        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string().map_err(transport)?;
        Ok(Response { status, body })
    }
}

#[cfg(test)]
pub(crate) mod mock {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::*;

    #[derive(Debug, Default)]
    pub struct MockHttp {
        script: RefCell<VecDeque<(String, Response)>>,
        pub seen: RefCell<Vec<Request>>,
    }

    impl MockHttp {
        pub fn push(&self, url_contains: &str, status: u16, body: &str) -> &Self {
            self.script.borrow_mut().push_back((
                url_contains.into(),
                Response {
                    status,
                    body: body.into(),
                },
            ));
            self
        }
    }

    impl HttpClient for MockHttp {
        fn send(&self, request: &Request) -> Result<Response, AuthError> {
            self.seen.borrow_mut().push(request.clone());
            let (needle, response) = self
                .script
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| panic!("unscripted request to {}", request.url));
            assert!(
                request.url.contains(&needle),
                "expected {needle}, got {}",
                request.url
            );
            Ok(response)
        }
    }
}

//! A small HTTP/1.1 mock server on a raw socket.
//!
//! It records the raw request target (so tests can check percent-encoding), serves a sequence of
//! canned answers, and can delay an answer or drop the connection. `wiremock` cannot drop a
//! connection after it reads the request, which the transport tests need.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// One canned answer.
#[derive(Debug, Clone, Default)]
pub struct MockResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub delay: Duration,
    pub drop_connection: bool,
}

impl MockResponse {
    pub fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![
                ("content-type".to_owned(), "application/json".to_owned()),
                ("x-klaracase-api-release".to_owned(), "0.95.0".to_owned()),
            ],
            body: body.as_bytes().to_vec(),
            ..Self::default()
        }
    }

    pub fn dropped() -> Self {
        Self {
            drop_connection: true,
            ..Self::default()
        }
    }
}

/// A request as the mock saw it.
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    /// The raw path, not decoded, without the query.
    pub path: String,
    /// The raw query string, without `?`.
    pub raw_query: Option<String>,
    /// Header names in lower case.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RecordedRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The query decoded into name and value pairs (`+` and `%20` both give a space).
    pub fn query_pairs(&self) -> Vec<(String, String)> {
        match &self.raw_query {
            None => Vec::new(),
            Some(raw) => url::form_urlencoded::parse(raw.as_bytes())
                .map(|(name, value)| (name.into_owned(), value.into_owned()))
                .collect(),
        }
    }
}

#[derive(Default)]
struct State {
    requests: Vec<RecordedRequest>,
    served: Vec<usize>,
    unmatched: usize,
    next: usize,
}

pub struct MockServer {
    addr: std::net::SocketAddr,
    state: Arc<Mutex<State>>,
    task: JoinHandle<()>,
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl MockServer {
    /// Starts a server on a free loopback port. The nth request gets `responses[n]`. With
    /// `repeat_last`, requests past the end get the last answer; otherwise they get HTTP 599.
    pub async fn start(responses: Vec<MockResponse>, repeat_last: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let state = Arc::new(Mutex::new(State::default()));
        let responses = Arc::new(responses);
        let task = tokio::spawn({
            let state = Arc::clone(&state);
            async move {
                loop {
                    let Ok((stream, _)) = listener.accept().await else {
                        return;
                    };
                    let state = Arc::clone(&state);
                    let responses = Arc::clone(&responses);
                    tokio::spawn(async move {
                        serve(stream, state, responses, repeat_last).await;
                    });
                }
            }
        });
        Self { addr, state, task }
    }

    pub fn base_url(&self) -> String {
        format!("http://{}/api/v1", self.addr)
    }

    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.state.lock().unwrap().requests.clone()
    }

    /// Indices of the canned answers served, in order.
    pub fn served(&self) -> Vec<usize> {
        self.state.lock().unwrap().served.clone()
    }

    /// How many requests found no answer left.
    pub fn unmatched(&self) -> usize {
        self.state.lock().unwrap().unmatched
    }
}

async fn serve(
    mut stream: TcpStream,
    state: Arc<Mutex<State>>,
    responses: Arc<Vec<MockResponse>>,
    repeat_last: bool,
) {
    let Some(request) = read_request(&mut stream).await else {
        return;
    };
    let chosen = {
        let mut guard = state.lock().unwrap();
        guard.requests.push(request);
        let index = guard.next;
        guard.next += 1;
        if index < responses.len() {
            guard.served.push(index);
            Some(responses[index].clone())
        } else if repeat_last && !responses.is_empty() {
            guard.served.push(responses.len() - 1);
            Some(responses[responses.len() - 1].clone())
        } else {
            guard.unmatched += 1;
            None
        }
    };
    let response =
        chosen.unwrap_or_else(|| MockResponse::json(599, r#"{"error":"no answer left"}"#));
    if response.drop_connection {
        drop(stream);
        return;
    }
    if !response.delay.is_zero() {
        tokio::time::sleep(response.delay).await;
    }
    let mut head = format!("HTTP/1.1 {} Mock\r\n", response.status);
    for (name, value) in &response.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!(
        "content-length: {}\r\nconnection: close\r\n\r\n",
        response.body.len()
    ));
    // The client may have given up already (timeout test); ignore write errors.
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(&response.body).await;
    let _ = stream.shutdown().await;
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

async fn read_request(stream: &mut TcpStream) -> Option<RecordedRequest> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    let head_end = loop {
        if let Some(position) = find(&buffer, b"\r\n\r\n") {
            break position;
        }
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_owned();
    let target = request_line.next()?.to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let header = |name: &str| {
        headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, v)| v.clone())
    };

    let mut body = buffer[head_end + 4..].to_vec();
    if header("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        loop {
            if find(&body, b"0\r\n\r\n").is_some() {
                break;
            }
            let read = stream.read(&mut chunk).await.ok()?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
        }
        body = decode_chunked(&body);
    } else {
        let length: usize = header("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        while body.len() < length {
            let read = stream.read(&mut chunk).await.ok()?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
        }
    }

    let (path, raw_query) = match target.split_once('?') {
        Some((path, query)) => (path.to_owned(), Some(query.to_owned())),
        None => (target, None),
    };
    Some(RecordedRequest {
        method,
        path,
        raw_query,
        headers,
        body,
    })
}

fn decode_chunked(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut rest = data;
    while let Some(line_end) = find(rest, b"\r\n") {
        let size_text = String::from_utf8_lossy(&rest[..line_end]);
        let Ok(size) = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
        else {
            break;
        };
        if size == 0 {
            break;
        }
        let start = line_end + 2;
        if rest.len() < start + size {
            break;
        }
        out.extend_from_slice(&rest[start..start + size]);
        rest = &rest[(start + size + 2).min(rest.len())..];
    }
    out
}

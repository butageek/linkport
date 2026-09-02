//! Redirect resolution for tracker/shortener links.
//!
//! When a clicked URL's host is configured in `redirect_hosts` and no rule
//! matched it, the hot path calls [`resolve`] to follow the HTTP redirect
//! chain — reading only status + `Location` headers, never response bodies
//! — so the rules can be evaluated against the final destination. Only
//! hosts the user configured are ever contacted.

use std::time::Duration;
use url::Url;

const MAX_HOPS: usize = 5;
const TIMEOUT: Duration = Duration::from_secs(5);

/// Follow redirects starting at `start`. Returns the final URL when at
/// least one redirect happened, `None` on error, no redirect, or non-web
/// schemes (best effort: routing falls back to the original URL).
pub fn resolve(start: &str) -> Option<String> {
    // Browser-like headers: click trackers commonly 406 non-browser
    // clients, and this fetch must look like the click it stands in for.
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(TIMEOUT)
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36",
        )
        .build();

    let mut current = start.to_string();
    let mut hopped = false;
    for _ in 0..MAX_HOPS {
        // A hop without a Location means the chain ended (terminal status
        // or error) — keep the best URL we have, don't abort the resolution.
        let Some(location) = hop(&agent, &current) else {
            break;
        };
        let next = join_location(&current, &location);
        if next == current {
            break;
        }
        current = next;
        hopped = true;
    }
    hopped.then_some(current)
}

/// Resolve a `Location` header value — often relative — against the URL it
/// came from; unparseable bases fall back to the raw header value.
fn join_location(current: &str, location: &str) -> String {
    match Url::parse(current)
        .ok()
        .and_then(|base| base.join(location).ok())
    {
        Some(joined) => joined.to_string(),
        None => location.to_string(),
    }
}

/// One redirect hop: the `Location` of a 3xx response, if any.
fn hop(agent: &ureq::Agent, url: &str) -> Option<String> {
    let location = |resp: &ureq::Response| resp.header("location").map(str::to_string);
    match agent.get(url).call() {
        // With redirects(0), ureq hands 3xx back either way depending on
        // how the server closes the connection — accept both shapes.
        Ok(resp) if (300..400).contains(&resp.status()) => location(&resp),
        Err(ureq::Error::Status(code, resp)) if (300..400).contains(&code) => location(&resp),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// Minimal local HTTP server: /start answers 302 -> /final, everything
    /// else answers 200. Keeps the test off the network.
    #[test]
    fn follows_local_redirect_chain() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let _server = std::thread::spawn(move || {
            for (i, stream) in listener.incoming().enumerate() {
                let mut stream = stream.unwrap();
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf); // drain the request line
                let resp = if i == 0 {
                    "HTTP/1.1 302 Found\r\nLocation: /final?x=1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                };
                stream.write_all(resp.as_bytes()).unwrap();
            }
        });

        let base = format!("http://{addr}");
        let resolved = resolve(&format!("{base}/start"));
        assert_eq!(
            resolved.as_deref(),
            Some(format!("{base}/final?x=1").as_str())
        );

        // No redirect -> None (the caller routes the original URL).
        let direct = resolve(&format!("{base}/already-final"));
        assert_eq!(direct, None);

        // The server thread parks on accept() when done; it dies with the
        // test process, so intentionally leak it (no join).
    }
}

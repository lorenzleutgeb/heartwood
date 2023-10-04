use std::num::NonZeroUsize;
use std::{collections::HashMap, process};
use time::Duration;

use radicle::prelude::Id;
use radicle_httpd as httpd;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = parse_options()?;

    // SAFETY: The logger is only initialized once.
    httpd::logger::init().unwrap();
    tracing::info!("version {}-{}", env!("CARGO_PKG_VERSION"), env!("GIT_HEAD"));

    match httpd::run(options).await {
        Ok(()) => {}
        Err(err) => {
            tracing::error!("Fatal: {:#}", err);
            process::exit(1);
        }
    }
    Ok(())
}

/// Parse command-line arguments into HTTP options.
fn parse_options() -> Result<httpd::Options, lexopt::Error> {
    use lexopt::prelude::*;

    let mut parser = lexopt::Parser::from_env();
    let mut listen = None;
    let mut aliases = HashMap::new();
    let mut cache = Some(httpd::DEFAULT_CACHE_SIZE);
    let mut session_expiry = httpd::api::auth::DEFAULT_AUTHORIZED_SESSIONS_EXPIRATION;

    while let Some(arg) = parser.next()? {
        match arg {
            Long("listen") => {
                let addr = parser.value()?.parse()?;
                listen = Some(addr);
            }
            Long("alias") | Short('a') => {
                let alias: String = parser.value()?.parse()?;
                let id: Id = parser.value()?.parse()?;

                aliases.insert(alias, id);
            }
            Long("cache") => {
                let size = parser.value()?.parse()?;
                cache = NonZeroUsize::new(size);
            }
            Long("session-expiry") | Short('e') => {
                let expiry_seconds: i64 = parser.value()?.parse()?;
                session_expiry = Duration::seconds(expiry_seconds);
            }
            Long("help") | Short('h') => {
                println!("usage: radicle-httpd [--listen <addr>] [--alias <name> <rid>] [--cache <size>] [--session-expiry <duration in seconds>] ..");
                process::exit(0);
            }
            _ => return Err(arg.unexpected()),
        }
    }
    Ok(httpd::Options {
        aliases,
        listen: listen.unwrap_or_else(|| ([127, 0, 0, 1], 8080).into()),
        cache,
        session_expiry,
    })
}

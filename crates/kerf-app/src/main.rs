//! `kerf-app` opens the desktop app. `kerf-app --serve 127.0.0.1:8787` instead
//! serves the same commands over HTTP for the web UI in a browser (`npm run
//! dev` in `ui/` talks to it).

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use kerf_app::bridge::Bridge;
use kerf_app::serve::HttpServer;
use kerf_app::{audio, shell};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None => shell::run().context("running the app"),
        Some("--serve") => {
            let addr = args
                .next()
                .context("--serve needs an address, e.g. 127.0.0.1:8787")?;
            let bridge = Arc::new(Bridge::new(audio::transport()));
            let server =
                HttpServer::bind(&addr, bridge).with_context(|| format!("binding {addr}"))?;
            println!("kerf-app serving http://{addr}/api/<command>");
            server.run();
            Ok(())
        }
        Some(other) => bail!("unknown argument `{other}`; use `--serve ADDR` or no arguments"),
    }
}

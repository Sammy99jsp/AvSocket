//!
//! Server-side utilities.
//!
//! ---
//! `proto.rs`
//! ```ignore
//! use avsocket::declare;
//!
//! declare!(
//!     /// Adds two `usize`s together.
//!     extern fn add(usize, usize) -> usize
//! );
//!
//! declare!(
//!     /// Subtracts the second `usize` from the first.
//!     extern fn sub(usize, usize) -> usize
//! );
//! ```
//! ---
//! `main.rs`
//! ```ignore
//!
//! mod proto;
//! 
//! use avsocket::server::{Handler, Server};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let path = /* ... */;
//!     let mut handler = Handler::default();
//!
//!     handler
//!         .add(proto::add, &|a, b| a + b)
//!         .add(proto::sub, &|a, b| a - b);
//!
//!     Server::run(&path, handler).await
//! }
//! ```
//!

use std::{collections::HashMap, fmt::Debug, fs, marker::Tuple, path::Path, sync::OnceLock};

use futures::{SinkExt, StreamExt};
use serde::{de::DeserializeOwned, Serialize};
use tokio::net::UnixListener;
use tokio_util::{
    bytes::Bytes,
    codec::{Framed, LengthDelimitedCodec},
};

use crate::{transport, Method};

///
/// Accepts raw version of Req, Res and will wrap normal callback.
///
type RawCallback = Box<dyn Fn(transport::Request<Vec<u8>>) -> transport::Response<Vec<u8>> + Sync>;

#[derive(Default)]
pub struct Handler(HashMap<&'static str, RawCallback>);

impl Debug for Handler {
    #[allow(clippy::format_collect)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Handler(\n{})",
            self.0
                .keys()
                .map(|k| format!("\t{k}\n"))
                .collect::<String>()
        )
    }
}

unsafe impl Send for Handler {}
impl Handler {
    pub fn add<Params, Returns, Impl>(
        &mut self,
        method: Method<Params, Returns>,
        implementation: &'static Impl,
    ) -> &mut Self
    where
        Params: Tuple + Serialize + DeserializeOwned,
        Returns: Serialize + DeserializeOwned,
        Impl: Fn<Params, Output = Returns> + Clone + Copy + Sync,
    {
        let _ = self.0.insert(
            method.0,
            Box::new(|req| {
                // TODO: More robust error handling.
                let body = bincode::deserialize::<Params>(req.body()).expect("Valid message sent");

                let res = implementation.call(body);
                let res = bincode::serialize(&res).expect("Valid serialization");

                req.reply(res)
            }),
        );

        self
    }

    pub fn handle(&self, input: impl AsRef<[u8]>) -> Option<Vec<u8>> {
        let input: transport::Request<Vec<u8>> = bincode::deserialize(input.as_ref()).ok()?;
        self.0
            .get(input.method())
            .map(|call| call(input.clone()))
            .and_then(|ref a| bincode::serialize(a).ok())
    }
}

pub struct Server;

impl Server {
    pub async fn run(path: impl AsRef<Path>, handler: Handler) -> anyhow::Result<()> {
        let path = path.as_ref();
        let _ = fs::create_dir_all(path.ancestors().nth(1).map_or(
            Err(anyhow::format_err!(
                "Socket cannot be at `/`. Try `/run/user/{{USER}}/...`"
            )),
            Ok,
        )?);

        let _ = fs::remove_file(path);

        let server = UnixListener::bind(path)?;

        println!("Listening on {}", path.to_str().unwrap());

        static HANDLER: OnceLock<Handler> = OnceLock::new();
        HANDLER.set(handler).unwrap();

        loop {
            match server.accept().await {
                Ok((s, _)) => {
                    tokio::spawn(async move {
                        let handler = HANDLER.get().unwrap();
                        let mut transport = Framed::new(s, LengthDelimitedCodec::new());

                        while let Some(Ok(thingy)) = transport.next().await {
                            if let Some(res) = handler.handle(thingy) {
                                if let Err(e) = transport.send(Bytes::from_iter(res)).await {
                                    eprintln!("Error occurred whilst replying to request:\n\t{e}.\nTerminating client connection.");
                                    break;
                                }
                            }
                        }
                    });
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}
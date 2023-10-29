//!
//! Utilities for client-side applications.
//!
//! ---
//! `proto.rs`
//! ```ignore
//! use avsocket::declare;
//!
//! declare!(extern fn add(usize, usize) -> usize);
//! declare!(extern fn sub(usize, usize) -> usize);
//! ```
//! ---
//! `main.rs`
//! ```ignore
//!
//!  mod proto;
//!
//! use avsocket::client::Dispatcher;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let path = /* ... */;
//!
//!     let mut dispatcher = Dispatcher::connect(&path).await?;    
//!
//!     let response = dispatcher.dispatch(proto::add(5,23)).await;
//!     println!("{response:?}");
//!
//!     Ok(())
//! }
//!```
//!
//!

use std::{marker::PhantomData, path::Path};

use crate::transport::{Request, Response};
use futures::{SinkExt, StreamExt};

use serde::de::DeserializeOwned;
use tokio::net::UnixStream;
use tokio_util::{
    bytes::Bytes,
    codec::{self, Framed, LengthDelimitedCodec},
};

pub struct Dispatcher(Framed<UnixStream, LengthDelimitedCodec>);

impl Dispatcher {
    pub async fn connect<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let con = tokio::net::UnixStream::connect(path).await?;

        let transport = codec::Framed::new(con, codec::LengthDelimitedCodec::new());
        Ok(Self(transport))
    }

    pub async fn dispatch<R: DeserializeOwned>(
        &mut self,
        req: (Request<Vec<u8>>, PhantomData<R>),
    ) -> anyhow::Result<R> {
        let transport = &mut self.0;
        let bin = Bytes::from_iter(bincode::serialize(&req)?);
        transport.send(bin).await?;

        let bin = transport.next().await.unwrap()?;
        let res = Response::from_bytes(bin)
            .map_or_else(
                || {
                    Err(anyhow::format_err!(
                        "Could not deserialize response from binary!"
                    ))
                },
                Ok,
            )?
            .convert_inner::<R>()
            .map_or_else(
                || {
                    Err(anyhow::format_err!(
                        "Could not deserialize reponse's body from binary!"
                    ))
                },
                Ok,
            )?;

        Ok(res.consume())
    }
}

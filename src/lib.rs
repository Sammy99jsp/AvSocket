#![feature(
    fs_try_exists,
    tuple_trait,
    unboxed_closures,
    associated_type_defaults,
    fn_traits
)]

use std::{
    fmt::Debug,
    marker::{PhantomData, Tuple},
};

use serde::{de::DeserializeOwned, Serialize};

///
/// Stores metadata about a method &mdash; its
/// * Name;
/// * Parameter types; and
/// * Return type.
/// 
#[derive(Debug, Clone, Copy)]
pub struct Method<P, R>(&'static str, PhantomData<P>, PhantomData<R>)
where
    P: Serialize + DeserializeOwned,
    R: Serialize + DeserializeOwned;

///
/// Converts a `fn` into a [Method].
///
#[allow(dead_code)]
pub const fn methodify<Params, Returns, F>(
    __func: &'static F,
    name: &'static str,
) -> Method<Params, Returns>
where
    Params: Tuple + Serialize + DeserializeOwned,
    Returns: Serialize + DeserializeOwned,
    F: Fn<Params, Output = Returns>,
{
    Method(name, PhantomData::<Params>, PhantomData::<Returns>)
}

///
/// Utility trait that links a [Method]'s parameter and return types.
///
pub trait MethodInfo<P, R>
where
    P: Serialize + DeserializeOwned,
    R: Serialize + DeserializeOwned,
{
    type Params = P;
    type Result = R;
}

impl<Params, Returns> MethodInfo<Params, Returns> for Method<Params, Returns>
where
    Params: Tuple + Serialize + DeserializeOwned,
    Returns: Serialize + DeserializeOwned,
{
    type Params = Params;
    type Result = Returns;
}

impl<Params, Returns> FnOnce<Params> for Method<Params, Returns>
where
    Params: Tuple + Serialize + DeserializeOwned,
    Returns: Serialize + DeserializeOwned,
{
    type Output = (transport::Request<Vec<u8>>, PhantomData<Returns>);

    extern "rust-call" fn call_once(self, args: Params) -> Self::Output {
        (
            transport::Request::new(self.0, bincode::serialize(&args).expect("Valid serialize")),
            PhantomData::<Returns>,
        )
    }
}

pub use macros::declare;

///
/// Utilities and middleware to help transport data.
///
/// * Makes use of `serde` and `bincode` to represent all the data as binary.
/// * Governs the structure of communication &mdash; [Request]s from the client,
/// followed by [Response]s from the server.
///
pub mod transport {
    use serde::{de::DeserializeOwned, Deserialize, Serialize};

    ///
    /// Client-to-server message.
    ///
    #[derive(Debug, Serialize, Deserialize)]
    pub struct Request<Body> {
        ///
        /// Unique UUID v4 for this request, to keep track of the server's response.
        ///
        id: String,

        ///
        /// Method's  ID.
        ///
        method: String,

        ///
        /// Payload.
        ///
        body: Body,
    }

    impl<Body> Clone for Request<Body>
    where
        Body: Clone,
    {
        fn clone(&self) -> Self {
            Self {
                id: self.id.clone(),
                method: self.method.clone(),
                body: self.body.clone(),
            }
        }
    }

    impl<Body> Request<Body> {
        pub fn new(label: impl ToString, body: Body) -> Self {
            Self {
                id: uuid::Uuid::new_v4().to_string(),
                method: label.to_string(),
                body,
            }
        }

        ///
        /// Serialize this [Request] as bytes using `bincode`
        /// (guaranteed not to fail... well *nearly*...).
        ///
        pub fn to_bytes(self) -> Vec<u8>
        where
            Body: Serialize,
        {
            let Self { id, method, body } = self;
            let tmp = Request {
                id,
                method,
                body: bincode::serialize(&body).expect("Valid serialize"),
            };
            bincode::serialize(&tmp).expect("Valid serialize Round 2")
        }

        ///
        /// Make a reply to this [Request] with the given body.
        ///
        pub fn reply<NewBody>(&self, body: NewBody) -> Response<NewBody> {
            Response {
                to: self.id.clone(),
                method: self.method.clone(),
                body,
            }
        }

        pub fn id(&self) -> &str {
            &self.id
        }

        pub fn body(&self) -> &Body {
            &self.body
        }

        pub fn method(&self) -> &str {
            &self.method
        }
    }

    impl Request<Vec<u8>> {
        ///
        /// Deserialize a raw request, with a type-erased body.
        ///
        /// This is done before deserializing the body seperately
        /// (for generic erasure reasons).
        ///
        pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Option<Self> {
            bincode::deserialize(bytes.as_ref()).ok()
        }

        ///
        /// Deserialize this [Request]'s inner body to the desired type.
        /// 
        pub fn convert_inner<Body: DeserializeOwned>(self) -> Option<Request<Body>> {
            let Self { id, method, body } = self;

            bincode::deserialize(&body)
                .map(|body| Request { id, method, body })
                .ok()
        }
    }

    impl Response<Vec<u8>> {
        ///
        /// Deserialize a raw [Response] into its type-erased form. 
        /// 
        pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Option<Self> {
            bincode::deserialize(bytes.as_ref()).ok()
        }

        ///
        /// Deserialize the inner type-erased body to a type.
        /// 
        pub fn convert_inner<Body: DeserializeOwned>(self) -> Option<Response<Body>> {
            let Self { to, method, body } = self;

            bincode::deserialize(&body)
                .map(|body| Response { to, method, body })
                .ok()
        }
    }

    ///
    /// Server-to-client message.
    /// 
    #[derive(Debug, Serialize, Deserialize)]
    pub struct Response<Body> {
        ///
        /// Same as the associated [Request]'s id field
        ///
        to: String,

        ///
        /// Method's  ID.
        ///
        method: String,

        ///
        /// Payload.
        ///
        body: Body,
    }

    impl<Body> Response<Body> {
        pub fn body(&self) -> &Body {
            &self.body
        }

        pub fn consume(self) -> Body {
            self.body
        }

        pub fn to(&self) -> &str {
            &self.to
        }


        ///
        /// Serialize this [Response] as bytes.
        /// 
        pub fn to_bytes(self) -> Vec<u8>
        where
            Body: Serialize,
        {
            let Self { to, method, body } = self;
            let tmp = Response {
                to,
                method,
                body: bincode::serialize(&body).expect("Valid serialize"),
            };
            bincode::serialize(&tmp).expect("Valid serialize Round 2")
        }
    }
}

#[cfg(test)]
pub mod transport_tests {
    use super::{methodify, Method};

    fn _method(_: String) -> String {
        unimplemented!();
    }

    #[allow(non_upper_case_globals)]
    pub const method: Method<(std::string::String,), std::string::String> =
        methodify(&_method, "method");

    #[test]
    fn client_side_call() {
        let _ = method("Apples".to_string());
    }
}

pub mod client;

pub mod server;

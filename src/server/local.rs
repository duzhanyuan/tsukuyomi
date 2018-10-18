//! A testing framework for Tsukuyomi.
//!
//! # Examples
//!
//! ```
//! # extern crate tsukuyomi;
//! # extern crate http;
//! # use tsukuyomi::app::App;
//! # use tsukuyomi::handler;
//! # use http::{Request, StatusCode, header};
//! use tsukuyomi::server::local::LocalServer;
//!
//! let app = App::builder()
//!     .route(("/hello", handler::wrap_ready(|_| "Hello")))
//!     .finish()
//!     .unwrap();
//!
//! // Create a local server from an App.
//! // The instance emulates the behavior of an HTTP service
//! // without the low level I/O.
//! let mut server = LocalServer::new(app).unwrap();
//!
//! // Emulate an HTTP request and retrieve its response.
//! let request = Request::get("/hello")
//!     .body(Default::default())
//!     .expect("should be a valid HTTP request");
//! let response = server.client()
//!     .perform(request)
//!     .expect("unrecoverable error");
//!
//! // Do some stuff...
//! assert_eq!(response.status(), StatusCode::OK);
//! assert!(response.headers().contains_key(header::CONTENT_TYPE));
//! assert_eq!(*response.body().to_bytes(), b"Hello"[..]);
//! ```

// TODO: emulates some behaviour of Hyper

use std::borrow::Cow;
use std::io;
use std::mem;
use std::str;

use bytes::Bytes;
use futures::{Async, Future, Poll, Stream};
use http::{Request, Response};
use hyper::service::{NewService, Service};
use hyper::Body;
use tokio::executor::thread_pool::Builder as ThreadPoolBuilder;
use tokio::runtime::{self, Runtime};

use super::CritError;

/// A local server which emulates an HTTP service without using the low-level transport.
///
/// This type wraps an `App` and a single-threaded Tokio runtime.
#[derive(Debug)]
pub struct LocalServer<S> {
    new_service: S,
    runtime: Runtime,
}

impl<S> LocalServer<S>
where
    S: NewService<ReqBody = Body, ResBody = Body>,
    S::Service: Send + 'static,
    S::Future: Send + 'static,
    S::InitError: Into<CritError> + 'static,
{
    /// Creates a new instance of `LocalServer` from a configured `App`.
    ///
    /// This function will return an error if the construction of the runtime is failed.
    pub fn new(new_service: S) -> io::Result<LocalServer<S>> {
        let mut pool = ThreadPoolBuilder::new();
        pool.pool_size(1);

        let runtime = runtime::Builder::new()
            .core_threads(1)
            .blocking_threads(1)
            .build()?;

        Ok(LocalServer {
            new_service,
            runtime,
        })
    }

    /// Create a `Client` associated with this server.
    pub fn client(&mut self) -> Client<'_, S::Service> {
        let service = self
            .runtime
            .block_on(self.new_service.new_service().map_err(Into::into))
            .expect("failed to construct a Service");
        Client {
            service,
            runtime: &mut self.runtime,
        }
    }
}

/// A type which emulates a connection to a peer.
#[derive(Debug)]
pub struct Client<'a, S> {
    service: S,
    runtime: &'a mut Runtime,
}

impl<'a, S> Client<'a, S>
where
    S: Service<ReqBody = Body, ResBody = Body>,
    S::Future: Send + 'static,
{
    /// Applies an HTTP request to this client and get its response.
    pub fn perform(&mut self, request: Request<Body>) -> Result<Response<Data>, CritError> {
        let future = TestResponseFuture::Initial(self.service.call(request));
        self.runtime.block_on(future)
    }

    /// Returns the reference to the underlying Tokio runtime.
    pub fn runtime(&mut self) -> &mut Runtime {
        &mut *self.runtime
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
#[derive(Debug)]
enum TestResponseFuture<F> {
    Initial(F),
    Receive(Response<Receive>),
    Done,
}

enum Polled {
    Response(Response<Body>),
    Received(Data),
}

impl<F> Future for TestResponseFuture<F>
where
    F: Future<Item = Response<Body>>,
    F::Error: Into<CritError>,
{
    type Item = Response<Data>;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let polled = match *self {
                TestResponseFuture::Initial(ref mut f) => {
                    Some(Polled::Response(try_ready!(f.poll().map_err(Into::into))))
                }
                TestResponseFuture::Receive(ref mut res) => {
                    Some(Polled::Received(try_ready!(res.body_mut().poll_ready())))
                }
                _ => unreachable!("unexpected state"),
            };

            match (mem::replace(self, TestResponseFuture::Done), polled) {
                (TestResponseFuture::Initial(..), Some(Polled::Response(response))) => {
                    *self = TestResponseFuture::Receive(response.map(Receive::new));
                }
                (TestResponseFuture::Receive(response), Some(Polled::Received(received))) => {
                    return Ok(response.map(|_| received).into())
                }
                _ => unreachable!("unexpected state"),
            }
        }
    }
}

// ==== Data ====

#[derive(Debug)]
pub(crate) struct Receive {
    body: Body,
    chunks: Vec<Bytes>,
}

impl Receive {
    fn new(body: Body) -> Receive {
        Receive {
            body,
            chunks: vec![],
        }
    }

    pub(crate) fn poll_ready(&mut self) -> Poll<Data, CritError> {
        while let Some(chunk) = try_ready!(self.body.poll()) {
            self.chunks.push(chunk.into());
        }
        let chunks = mem::replace(&mut self.chunks, vec![]);
        Ok(Async::Ready(Data(chunks)))
    }
}

/// A type representing a received HTTP message data from the server.
///
/// This type is usually used by the testing framework.
#[derive(Debug)]
pub struct Data(Vec<Bytes>);

#[allow(missing_docs)]
impl Data {
    pub fn is_sized(&self) -> bool {
        false
    }

    pub fn is_chunked(&self) -> bool {
        !self.is_sized()
    }

    pub fn content_length(&self) -> Option<usize> {
        None
    }

    pub fn as_chunks(&self) -> Option<&[Bytes]> {
        Some(&self.0[..])
    }

    pub fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(self.0.iter().fold(Vec::new(), |mut acc, chunk| {
            acc.extend_from_slice(&*chunk);
            acc
        }))
    }

    pub fn to_utf8(&self) -> Result<Cow<'_, str>, str::Utf8Error> {
        match self.to_bytes() {
            Cow::Borrowed(bytes) => str::from_utf8(bytes).map(Cow::Borrowed),
            Cow::Owned(bytes) => String::from_utf8(bytes)
                .map_err(|e| e.utf8_error())
                .map(Cow::Owned),
        }
    }
}

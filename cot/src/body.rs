use std::fmt::{Debug, Formatter};
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use http_body::{Frame, SizeHint};
use http_body_util::combinators::BoxBody;
use sync_wrapper::SyncWrapper;

use crate::error::ErrorRepr;
use crate::{Error, Result};

/// A type that represents an HTTP request or response body.
///
/// This type is used to represent the body of an HTTP request/response. It can
/// be either a fixed body (e.g., a string or a byte array) or a streaming body
/// (e.g., a large file or a database query result).
///
/// # Examples
///
/// ```
/// use cot::Body;
///
/// let body = Body::fixed("Hello, world!");
/// let body = Body::streaming(futures::stream::once(async { Ok("Hello, world!".into()) }));
/// ```
#[derive(Debug)]
pub struct Body {
    pub(crate) inner: BodyInner,
}

pub(crate) enum BodyInner {
    Fixed(Bytes),
    Streaming(SyncWrapper<Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>>),
    Axum(SyncWrapper<axum::body::Body>),
    Wrapper(BoxBody<Bytes, Error>),
}

impl Debug for BodyInner {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fixed(data) => f.debug_tuple("Fixed").field(data).finish(),
            Self::Streaming(_) => f.debug_tuple("Streaming").field(&"...").finish(),
            Self::Axum(axum_body) => f.debug_tuple("Axum").field(axum_body).finish(),
            Self::Wrapper(_) => f.debug_tuple("Wrapper").field(&"...").finish(),
        }
    }
}

impl Body {
    #[must_use]
    const fn new(inner: BodyInner) -> Self {
        Self { inner }
    }

    /// Create an empty body.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::Body;
    ///
    /// let body = Body::empty();
    /// ```
    #[must_use]
    pub const fn empty() -> Self {
        Self::new(BodyInner::Fixed(Bytes::new()))
    }

    /// Create a body instance with the given fixed data.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::Body;
    ///
    /// let body = Body::fixed("Hello, world!");
    /// ```
    #[must_use]
    pub fn fixed<T: Into<Bytes>>(data: T) -> Self {
        Self::new(BodyInner::Fixed(data.into()))
    }

    /// Create a body instance from a stream of data.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_stream::stream;
    /// use cot::Body;
    ///
    /// let stream = stream! {
    ///    yield Ok("Hello, ".into());
    ///    yield Ok("world!".into());
    /// };
    /// let body = Body::streaming(stream);
    /// ```
    #[must_use]
    pub fn streaming<T: Stream<Item = Result<Bytes>> + Send + 'static>(stream: T) -> Self {
        Self::new(BodyInner::Streaming(SyncWrapper::new(Box::pin(stream))))
    }

    /// Convert this [`Body`] instance into a byte array.
    ///
    /// This method reads the entire body into memory and returns it as a byte
    /// array. Note that if the body is too large, this method can consume a lot
    /// of memory. For a way to read the body while limiting the memory usage,
    /// see [`Self::into_bytes_limited`].
    ///
    /// # Errors
    ///
    /// This method returns an error if reading the body fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::Body;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let body = Body::fixed("Hello, world!");
    /// let bytes = body.into_bytes().await?;
    /// assert_eq!(bytes, "Hello, world!".as_bytes());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn into_bytes(self) -> Result<Bytes> {
        self.into_bytes_limited(usize::MAX).await
    }

    /// Convert this [`Body`] instance into a byte array.
    ///
    /// This is a version of [`Self::into_bytes`] that allows you to limit the
    /// amount of memory used to read the body. If the body is larger than
    /// the limit, an error is returned.
    ///
    /// # Errors
    ///
    /// This method returns an error if reading the body fails.
    ///
    /// If the body is larger than the limit, an error is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::Body;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let body = Body::fixed("Hello, world!");
    /// let bytes = body.into_bytes_limited(32).await?;
    /// assert_eq!(bytes, "Hello, world!".as_bytes());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn into_bytes_limited(self, limit: usize) -> Result<Bytes> {
        use http_body_util::BodyExt;

        Ok(http_body_util::Limited::new(self, limit)
            .collect()
            .await
            .map(http_body_util::Collected::to_bytes)
            .map_err(|source| ErrorRepr::ReadRequestBody { source })?)
    }

    #[must_use]
    pub(crate) fn axum(inner: axum::body::Body) -> Self {
        Self::new(BodyInner::Axum(SyncWrapper::new(inner)))
    }

    #[must_use]
    pub(crate) fn wrapper(inner: BoxBody<Bytes, Error>) -> Self {
        Self::new(BodyInner::Wrapper(inner))
    }
}

impl Default for Body {
    fn default() -> Self {
        Self::empty()
    }
}

impl http_body::Body for Body {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<std::result::Result<Frame<Self::Data>, Self::Error>>> {
        match self.get_mut().inner {
            BodyInner::Fixed(ref mut data) => {
                if data.is_empty() {
                    Poll::Ready(None)
                } else {
                    let data = std::mem::take(data);
                    Poll::Ready(Some(Ok(Frame::data(data))))
                }
            }
            BodyInner::Streaming(ref mut stream) => {
                let stream = Pin::as_mut(stream.get_mut());
                match stream.poll_next(cx) {
                    Poll::Ready(Some(result)) => Poll::Ready(Some(result.map(Frame::data))),
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                }
            }
            BodyInner::Axum(ref mut axum_body) => {
                let axum_body = axum_body.get_mut();
                Pin::new(axum_body).poll_frame(cx).map_err(|error| {
                    ErrorRepr::ReadRequestBody {
                        source: Box::new(error),
                    }
                    .into()
                })
            }
            BodyInner::Wrapper(ref mut http_body) => {
                Pin::new(http_body).poll_frame(cx).map_err(|error| {
                    ErrorRepr::ReadRequestBody {
                        source: Box::new(error),
                    }
                    .into()
                })
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        match &self.inner {
            BodyInner::Fixed(data) => data.is_empty(),
            BodyInner::Streaming(_) | BodyInner::Axum(_) => false,
            BodyInner::Wrapper(http_body) => http_body.is_end_stream(),
        }
    }

    fn size_hint(&self) -> SizeHint {
        match &self.inner {
            BodyInner::Fixed(data) => SizeHint::with_exact(data.len() as u64),
            BodyInner::Streaming(_) | BodyInner::Axum(_) => SizeHint::new(),
            BodyInner::Wrapper(http_body) => http_body.size_hint(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use futures::stream;
    use http_body::Body as HttpBody;

    use super::*;

    #[test]
    fn body_empty() {
        let body = Body::empty();
        if let BodyInner::Fixed(data) = body.inner {
            assert!(data.is_empty());
        } else {
            panic!("Body::empty should create a fixed empty body");
        }
    }

    #[test]
    fn body_fixed() {
        let content = "Hello, world!";
        let body = Body::fixed(content);
        if let BodyInner::Fixed(data) = body.inner {
            assert_eq!(data, Bytes::from(content));
        } else {
            panic!("Body::fixed should create a fixed body with the given content");
        }
    }

    #[tokio::test]
    async fn body_streaming() {
        let stream = stream::once(async { Ok(Bytes::from("Hello, world!")) });
        let body = Body::streaming(stream);
        if let BodyInner::Streaming(_) = body.inner {
            // Streaming body created successfully
        } else {
            panic!("Body::streaming should create a streaming body");
        }
    }

    #[tokio::test]
    async fn http_body_poll_frame_fixed() {
        let content = "Hello, world!";
        let mut body = Body::fixed(content);
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(frame))) => {
                assert_eq!(frame.into_data().unwrap(), Bytes::from(content));
            }
            _ => panic!("Body::fixed should return the content in poll_frame"),
        }

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(None) => {} // End of stream
            _ => panic!("Body::fixed should return None after the content is consumed"),
        }
    }

    #[tokio::test]
    async fn http_body_poll_frame_streaming() {
        let content = "Hello, world!";
        let mut body = Body::streaming(stream::once(async move { Ok(Bytes::from(content)) }));
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(frame))) => {
                assert_eq!(frame.into_data().unwrap(), Bytes::from(content));
            }
            _ => panic!("Body::fixed should return the content in poll_frame"),
        }

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(None) => {} // End of stream
            _ => panic!("Body::fixed should return None after the content is consumed"),
        }
    }

    #[test]
    fn http_body_is_end_stream() {
        let body = Body::empty();
        assert!(body.is_end_stream());

        let body = Body::fixed("Hello, world!");
        assert!(!body.is_end_stream());
    }

    #[test]
    fn http_body_size_hint() {
        let body = Body::empty();
        assert_eq!(body.size_hint().exact(), Some(0));

        let content = "Hello, world!";
        let body = Body::fixed(content);
        assert_eq!(body.size_hint().exact(), Some(content.len() as u64));
    }
}

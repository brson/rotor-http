use std::io::Write;
use std::any::Any;

use rotor_stream::Buf;
use hyper::method::Method;
use hyper::status::StatusCode;
use hyper::version::HttpVersion as Version;
use hyper::header::{Header, HeaderFormat, HeaderFormatter};
use hyper::header::{ContentLength, TransferEncoding, Encoding};


quick_error! {
    #[derive(Debug)]
    pub enum HeaderError {
        DuplicateContentLength {
            description("Content-Length is added twice")
        }
        DuplicateTransferEncoding {
            description("Transfer-Encoding is added twice")
        }
        TransferEncodingAfterContentLength {
            description("Transfer encoding added when Content-Length is \
                already specified")
        }
        ContentLengthAfterTransferEncoding {
            description("Content-Length added after Transfer-Encoding")
        }
        UnknownTransferEncoding {
            description("Unknown Transfer-Encoding, only chunked is supported")
        }
        CantDetermineBodySize {
            description("Neither Content-Length nor TransferEncoding \
                is present in the headers")
        }
    }
}

#[derive(Debug)]
pub enum MessageState {
    /// Nothing has been sent
    ResponseStart { version: Version, body: Body },
    RequestStart,
    /// Status line is already in the buffer
    Headers { body: Body, chunked: bool, request: bool,
              content_length: Option<u64> },
    ZeroBodyMessage,  // When response body is Denied
    IgnoredBody, // When response body is Ignored
    FixedSizeBody(u64),
    ChunkedBody,
    Done,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Body {
    Normal,
    Ignored,  // HEAD requests, 304 responses
    Denied,  // 101, 204 responses (100 too if it is used here)
}

/// Represents both request message and response message
///
/// Specific wrappers are exposed in `server` and `client` modules.
/// This type is private for the crate
pub struct Message<'a>(&'a mut Buf, MessageState);

impl MessageState {
    pub fn with<'x, I>(self, out_buf: &'x mut Buf) -> I
        where I: From<Message<'x>>
    {
        Message(out_buf, self).into()
    }
}

impl<'a> Message<'a> {
    /// Write status line
    ///
    /// This puts status line into a buffer immediately. If you don't
    /// continue with request it will be sent to the network shortly.
    ///
    /// # Panics
    ///
    /// When status line is already written. It's expected that your request
    /// handler state machine will never call the method twice.
    ///
    /// When status is 10x we don't assert yet
    pub fn response_status(&mut self, code: StatusCode) {
        use hyper::status::StatusCode::*;
        use self::Body::*;
        use self::MessageState::*;
        match self.1 {
            ResponseStart { version, mut body } => {
                // Note we don't expect code 100 and 102 here, but
                // we don't assert on that for now. The point is that
                // responses 100 and 102 are interim. 100 is generated by
                // rotor-http itself and 102 should probably too. Or we
                // will have a special method in request for it, because
                // request will contain another (real) response status here.
                //
                // TODO(tailhook) should we assert?
                //
                write!(self.0, "{} {}\r\n", version, code).unwrap();
                if matches!(code, SwitchingProtocols|NoContent) {
                    body = Denied;
                } else if body == Normal && code == NotModified {
                    body = Ignored;
                }
                self.1 = Headers { body: body, request: false,
                                   content_length: None, chunked: false };
            }
            ref state => {
                panic!("Called status() method on response in a state {:?}",
                       state)
            }
        }
    }
    /// Write request line
    ///
    /// This puts request line into a buffer immediately. If you don't
    /// continue with request it will be sent to the network shortly.
    ///
    /// # Panics
    ///
    /// When request line is already written. It's expected that your request
    /// handler state machine will never call the method twice.
    pub fn request_line(&mut self, method: Method, uri: &str, version: Version)
    {
        use self::Body::*;
        use self::MessageState::*;
        match self.1 {
            RequestStart => {
                write!(self.0, "{} {} {}\r\n", method, uri, version).unwrap();
                // It's common to allow request body for GET, is it so
                // expected for the HEAD too? Other methods?
                self.1 = Headers { body: Normal, request: true,
                                   content_length: None, chunked: false };
            }
            ref state => {
                panic!("Called status() method on response in a state {:?}",
                       state)
            }
        }
    }

    /// Add header to message
    ///
    /// Header is written into the output buffer immediately. And is sent
    /// as soon as the next loop iteration
    ///
    /// Fails when invalid combination of headers is encountered. Note we
    /// don't validate all the headers but only security-related ones like
    /// double content-length and content-length with the combination of
    /// transfer-encoding.
    ///
    /// We return Result here to make implementing proxies easier. In the
    /// application handler it's okay to unwrap the result and to get
    /// a meaningful panic (that is basically an assertion).
    ///
    /// # Panics
    ///
    /// * Panics when add_header is called in the wrong state.
    /// * Panics on unsupported transfer encoding
    ///
    pub fn add_header<H: Header+HeaderFormat>(&mut self, header: H)
        -> Result<(), HeaderError>
    {
        use self::MessageState::*;
        use self::HeaderError::*;
        match self.1 {
            Headers { ref mut content_length, ref mut chunked, .. } => {
                match Any::downcast_ref::<ContentLength>(&header) {
                    Some(&ContentLength(ln)) => {
                        if *chunked {
                            return Err(ContentLengthAfterTransferEncoding);
                        }
                        if content_length.is_some() {
                            return Err(DuplicateContentLength);
                        }
                        *content_length = Some(ln);
                    }
                    None => {}
                }
                match Any::downcast_ref::<TransferEncoding>(&header) {
                    Some(te) if te[..] == [Encoding::Chunked] => {
                        if *chunked {
                            return Err(DuplicateTransferEncoding);
                        }
                        if content_length.is_some() {
                            return Err(TransferEncodingAfterContentLength);
                        }
                        *chunked = true;
                    }
                    Some(_) => {
                        return Err(UnknownTransferEncoding);
                    }
                    None => {}
                }
                write!(self.0, "{}: {}\r\n",
                    H::header_name(),
                    HeaderFormatter(&header)).unwrap();
                Ok(())
            }
            ref state => {
                panic!("Called add_header() method on response in a state {:?}",
                       state)
            }
        }
    }
    /// Returns true if at least `status()` method has been called
    ///
    /// This is mostly useful to find out whether we can build an error page
    /// or it's already too late.
    pub fn is_started(&self) -> bool {
        !matches!(self.1,
            MessageState::RequestStart |
            MessageState::ResponseStart { .. })
    }
    /// Checks the validity of headers. And returns `true` if entity
    /// body is expected.
    ///
    /// Specifically `false` is returned when status is 101, 204, 304 or the
    /// request is HEAD. Which means in both cases where response body is
    /// either ignored (304, HEAD) or is denied by specification. But not
    /// when response is zero-length.
    ///
    /// Similarly to `add_header()` it's fine to `unwrap()` here, unless you're
    /// doing some proxying.
    ///
    /// # Panics
    ///
    /// Panics when response is in a wrong state
    pub fn done_headers(&mut self) -> Result<bool, HeaderError> {
        use self::Body::*;
        use self::MessageState::*;
        let result = match self.1 {
            Headers { body: Ignored, .. } => {
                self.1 = IgnoredBody;
                Ok(false)
            }
            Headers { body: Denied, .. } => {
                self.1 = ZeroBodyMessage;
                Ok(false)
            }
            Headers { body: Normal, content_length: Some(cl),
                      chunked: false, request: _ }
            => {
                self.1 = FixedSizeBody(cl);
                Ok(true)
            }
            Headers { body: Normal, content_length: None, chunked: true,
                      request: _ }
            => {
                self.1 = ChunkedBody;
                Ok(true)
            }
            Headers { content_length: Some(_), chunked: true, .. }
            => unreachable!(),
            Headers { body: Normal, content_length: None, chunked: false,
                      request: true }
            => {
                self.1 = ZeroBodyMessage;
                Ok(false)
            }
            Headers { body: Normal, content_length: None, chunked: false,
                      request: false }
            => Err(HeaderError::CantDetermineBodySize),
            ref state => {
                panic!("Called done_headers() method on  in a state {:?}",
                       state)
            }
        };
        self.0.write(b"\r\n").unwrap();
        result
    }
    /// Write a chunk of the body
    ///
    /// Works both for fixed-size body and chunked body.
    ///
    /// For the chunked body each chunk is put into the buffer immediately
    /// prefixed by chunk size.
    ///
    /// For both modes chunk is put into the buffer, but is only sent when
    /// rotor-stream state machine is reached. So you may put multiple chunks
    /// into the buffer quite efficiently.
    ///
    /// For Ignored body you can `write_body` any number of times, it's just
    /// ignored. But it's more efficient to check it with `needs_body()`
    ///
    /// # Panics
    ///
    /// When response is in wrong state. Or there is no headers which
    /// determine response body length (either Content-Length or
    /// Transfer-Encoding)
    pub fn write_body(&mut self, data: &[u8]) {
        use self::MessageState::*;
        match self.1 {
            ZeroBodyMessage => {
                if data.len() != 0 {
                    panic!("Non-zero data length for the response where \
                            the response body is denied (101, 204)");
                }
            }
            FixedSizeBody(ref mut x) => {
                if data.len() as u64 > *x {
                    panic!("Fixed size response error. \
                        Bytes left {} but got additional {}", x, data.len());
                }
                self.0.write(data).unwrap();
                *x -= data.len() as u64;
            }
            ChunkedBody => {
                write!(self.0, "{:x}\r\n", data.len()).unwrap();
                self.0.write(data).unwrap();
            }
            ref state => {
                panic!("Called write_body() method on response \
                    in a state {:?}", state)
            }
        }
    }
    /// Returns true if `done()` method is already called and everything
    /// was okay.
    pub fn is_complete(&self) -> bool {
        matches!(self.1, MessageState::Done)
    }
    /// Writes needed final finalization data into the buffer and asserts
    /// that response is in the appropriate state for that.
    ///
    /// The method may be called multiple times
    ///
    /// # Panics
    ///
    /// When the response is in the wrong state or when Content-Length bytes
    /// are not written yet
    pub fn done(&mut self) {
        use self::MessageState::*;
        match self.1 {
            ChunkedBody => {
                self.0.write(b"0\r\n").unwrap();
                self.1 = Done;
            }
            FixedSizeBody(0) => self.1 = Done,
            ZeroBodyMessage => self.1 = Done,
            IgnoredBody => self.1 = Done,
            Done => {}  // multiple invocations are okay
            ref state => {
                panic!("Called done() method on response in a state {:?}",
                       state);
            }
        }
    }

    pub fn state(self) -> MessageState {
        self.1
    }
    pub fn decompose(self) -> (&'a mut Buf, MessageState) {
        (self.0, self.1)
    }

    /// This is used for error pages, where it's impossible to parse input
    /// headers (i.e. get Head object needed for `Message::new`)
    pub fn simple<'x>(out_buf: &'x mut Buf, is_head: bool) -> Message<'x>
    {
        use self::Body::*;
        Message(out_buf, MessageState::ResponseStart {
            body: if is_head { Ignored } else { Normal },
            // Always assume HTTP/1.0 when version is unknown
            version: Version::Http10,
        })
    }
}

#[cfg(test)]
mod test {
    use rotor_stream::Buf;
    use hyper::method::Method;
    use hyper::status::StatusCode;
    use hyper::header::ContentLength;
    use hyper::version::HttpVersion;
    use super::{Message, MessageState, Body};

    #[test]
    fn message_size() {
        // Just to keep track of size of structure
        assert_eq!(::std::mem::size_of::<MessageState>(), 24);
    }

    fn do_request<F: FnOnce(Message)>(fun: F) -> Buf {
        let mut buf = Buf::new();
        fun(MessageState::RequestStart.with(&mut buf));
        return buf;
    }
    fn do_response10<F: FnOnce(Message)>(fun: F) -> Buf {
        let mut buf = Buf::new();
        fun(MessageState::ResponseStart {
            version: HttpVersion::Http10,
            body: Body::Normal,
        }.with(&mut buf));
        return buf;
    }

    #[test]
    fn minimal_request() {
        assert_eq!(&do_request(|mut msg| {
            msg.request_line(Method::Get, "/", HttpVersion::Http10);
            msg.done_headers().unwrap();
            msg.done();
        })[..], "GET / HTTP/1.0\r\n\r\n".as_bytes());
    }

    #[test]
    fn minimal_response() {
        assert_eq!(&do_response10(|mut msg| {
            msg.response_status(StatusCode::Ok);
            msg.add_header(ContentLength(0)).unwrap();
            msg.done_headers().unwrap();
            msg.done();
        })[..], "HTTP/1.0 200 OK\r\nContent-Length: 0\r\n\r\n".as_bytes());
    }
}

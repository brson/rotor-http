extern crate rotor;
extern crate rotor_stream;
extern crate rotor_http;
extern crate time;


use std::env;
use std::thread;
use rotor::Scope;
use rotor_http::status::StatusCode::{self, NotFound};
use rotor_http::header::ContentLength;
use rotor_http::uri::RequestUri;
use rotor_stream::{Deadline, Accept, Stream};
use rotor_http::server::{RecvMode, Server, Head, Response, Parser};
use rotor_http::server::{Context as HttpContext};
use rotor::mio::tcp::TcpListener;
use time::Duration;


struct Context {
    counter: usize,
}

trait Counter {
    fn increment(&mut self);
    fn get(&self) -> usize;
}

impl Counter for Context {
    fn increment(&mut self) { self.counter += 1; }
    fn get(&self) -> usize { self.counter }
}

impl rotor_http::server::Context for Context {
    // default impl is okay
    fn byte_timeout(&self) -> Duration {
        Duration::seconds(1000)
    }
}


#[derive(Debug, Clone)]
enum HelloWorld {
    Start,
    Hello,
    GetNum,
    HelloName(String),
    PageNotFound,
}

fn send_string(res: &mut Response, data: &[u8]) {
    res.status(StatusCode::Ok);
    res.add_header(ContentLength(data.len() as u64)).unwrap();
    res.done_headers().unwrap();
    res.write_body(data);
    res.done();
}

impl Server for HelloWorld {
    type Context = Context;
    fn headers_received(_head: &Head, _scope: &mut Scope<Context>)
        -> Result<(Self, RecvMode, Deadline), StatusCode>
    {
        Ok((HelloWorld::Start, RecvMode::Buffered(1024),
            Deadline::now() + Duration::seconds(10)))
    }
    fn request_start(self, head: Head, _res: &mut Response,
        scope: &mut Scope<Context>)
        -> Option<Self>
    {
        use self::HelloWorld::*;
        scope.increment();
        match head.uri {
            RequestUri::AbsolutePath(ref p) if &p[..] == "/" => {
                Some(Hello)
            }
            RequestUri::AbsolutePath(ref p) if &p[..] == "/num"
            => {
                Some(GetNum)
            }
            RequestUri::AbsolutePath(p) => {
                Some(HelloName(p[1..].to_string()))
            }
            _ => {
                Some(PageNotFound)
            }
        }
    }
    fn request_received(self, _data: &[u8], res: &mut Response,
        scope: &mut Scope<Context>)
        -> Option<Self>
    {
        use self::HelloWorld::*;
        match self {
            Hello => {
                send_string(res, b"Hello World!");
            }
            GetNum => {
                send_string(res,
                    format!("This host has been visited {} times",
                        scope.get())
                    .as_bytes());
            }
            HelloName(name) => {
                send_string(res, format!("Hello {}!", name).as_bytes());
            }
            Start => unreachable!(),
            PageNotFound => {
                scope.emit_error_page(NotFound, res);
            }
        }
        None
    }
    fn request_chunk(self, _chunk: &[u8], _response: &mut Response,
        _scope: &mut Scope<Context>)
        -> Option<Self>
    {
        unreachable!();
    }

    /// End of request body, only for Progressive requests
    fn request_end(self, _response: &mut Response, _scope: &mut Scope<Context>)
        -> Option<Self>
    {
        unreachable!();
    }

    fn timeout(self, _response: &mut Response, _scope: &mut Scope<Context>)
        -> Option<(Self, Deadline)>
    {
        unimplemented!();
    }
    fn wakeup(self, _response: &mut Response, _scope: &mut Scope<Context>)
        -> Option<Self>
    {
        unimplemented!();
    }
}

fn main() {
    let lst = TcpListener::bind(&"127.0.0.1:3000".parse().unwrap()).unwrap();
    let threads = env::var("THREADS").unwrap_or("2".to_string())
        .parse().unwrap();
    let mut children = Vec::new();
    for _ in 0..threads {
        let listener = lst.try_clone().unwrap();
        children.push(thread::spawn(move || {
            let mut event_loop = rotor::EventLoop::new().unwrap();
            let mut handler = rotor::Handler::new(Context {
                counter: 0,
            }, &mut event_loop);
            let ok = handler.add_machine_with(&mut event_loop, |scope| {
                Accept::<Stream<Parser<HelloWorld, _>>, _>::new(
                        listener, scope)
            }).is_ok();
            assert!(ok);
            event_loop.run(&mut handler).unwrap();
        }));
    }
    for child in children {
        child.join().unwrap();
    }
}

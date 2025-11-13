use std::env;
use futures_util::TryFutureExt;
use http_body_util::Full;
use hyper::service::service_fn;
use hyper::{
    body::{Bytes, Incoming},
    server::conn::http1,
    HeaderMap, Request, Response,
};
use hyper_util::rt::TokioIo;
use std::fs::File;
use std::net::SocketAddr;
use std::path::Path;
use tokio::net::TcpListener;
mod mode;
use mode::*;

async fn hello(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, String> {
    let file = File::open(Path::new("route_response.json")).map_err(|e| e.to_string())?;
    let json: Vec<RouteResponse> = serde_json::from_reader(file).map_err(|e| e.to_string())?;
    let res = json.iter()
        .find(|e| req.uri().path().contains(&e.url) && e.method.eq_ignore_ascii_case(req.method().as_str()))
        .ok_or(format!("no route response found"))?;
    let mut respones = Response::new(Full::new(serde_json::to_vec(&res.response).unwrap().into()));
    respones.headers_mut().extend(req.headers().clone());
    if let Some(map) = res.headers.clone() {
        if !map.is_empty() {
            let headers: HeaderMap = (&map).try_into().expect("valid headers");
            respones.headers_mut().extend(headers);
        }
    }
    Ok(respones.into())
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let port:u16 = env::args().nth(1).unwrap_or("8080".to_string()).parse().unwrap();
    println!("Listening on port {}", port);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).map_err(|e| e.to_string()).await?;
    loop {
        let (stream, _) = listener.accept().map_err(|e| e.to_string()).await?;
        let io = TokioIo::new(stream);
        tokio::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(hello))
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}

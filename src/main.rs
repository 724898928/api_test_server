use futures_util::TryFutureExt;
use http_body_util::Full;
use hyper::service::service_fn;
use hyper::{
    body::{Bytes, Incoming},
    server::conn::http1,
    HeaderMap, Request, Response,
};
use hyper_util::rt::TokioIo;
use regex::Regex;
use std::fs::File;
use std::net::SocketAddr;
use std::path::Path;
use tokio::net::TcpListener;
mod mode;
use mode::*;

fn regex_url(url: &str, re_url: &str) -> bool {
    let mut re_url = re_url.replace("*", "[^/]+");
    re_url = format!("^{}$", re_url);
    let regex = Regex::new(re_url.as_str()).unwrap();
    let url = url.split("?").next().unwrap();
    let res = regex.find(url).is_some();
    println!("find:{}, regex_url: {} , url: {}",res, re_url, url);
    res
}

async fn hello(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, String> {
    let file = File::open(Path::new("route_response.json")).map_err(|e| e.to_string())?;
    let json: Vec<RouteResponse> = serde_json::from_reader(file).map_err(|e| e.to_string())?;
    let scheme = &req.uri().scheme();
  //  println!("req.uri():{:?}", &req.uri(),);
    println!("req.uri().path():{:?}  scheme: {:?}", &req.uri().path(), scheme);
    let res = json
        .iter()
        .find(|e| {
            regex_url(req.uri().path(), &e.url)
                && e.method.eq_ignore_ascii_case(req.method().as_str())
        })
        .ok_or(format!("no route response found"))?;
    println!("response  :{:?}", &res.response);
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
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
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
    // let url = "/api/v2/firmware/uploadauthorize/log.txt?serial=2205H9HD9990&requestId=550e8400-e29b-41d4-a716-446655440000";
    // let res = regex_url(url, "/api/v2/firmware/uploadauthorize/*.txt");
    // println!("res.url():{:?}", &res);

    Ok(())
}

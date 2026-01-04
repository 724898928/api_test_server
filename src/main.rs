use futures_util::TryFutureExt;
use http_body_util::Full;
use hyper::service::service_fn;
use hyper::{body::{ Incoming, Body}, server::conn::http1, HeaderMap, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use regex::Regex;
use std::net::SocketAddr;
use std::path::Path;
use bytes::Bytes;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
mod mode;
use mode::*;

fn regex_url(url: &str, re_url: &str) -> bool {
    let mut re_url = re_url.replace("*", "[^/]+");
    re_url = format!("^{}$", re_url);
    let regex = Regex::new(re_url.as_str()).unwrap();
    let url = url.split("?").next().unwrap();
    let res = regex.find(url).is_some();
    //println!("find:{}, regex_url: {} , url: {}",res, re_url, url);
    res
}

async fn download_with_resume(path: &str) -> Result<Response<Full<Bytes>>, String> {
    let mut respones = Response::new(Full::new("Invalid path".into()));
    if path.contains("..") || path.contains("~") {
        return Ok(respones);
    }
    // 提取文件名
    let mut filename = path.trim_start_matches("/download/").to_string();
    filename = format!("example/{}",filename);
    let file_path = Path::new(&filename);
    if file_path.exists() {
        let mut file = File::open(file_path).map_err(|err| err.to_string()).await?;
        // 读取文件内容
        let mut contents = Vec::new();
        if let Err(e) = file.read_to_end(&mut contents).await {
            let response = Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::from(format!("读取文件失败: {}", e)))
                .unwrap();
            return Ok(response);
        }

        // 获取 MIME 类型
        let mime_type = mime_guess::from_path(&file_path)
            .first_or_octet_stream()
            .to_string();

        // 创建响应
        let response = Response::builder()
            .header("Content-Type", mime_type)
            .header("Content-Disposition", format!("attachment; filename=\"{}\"", filename))
            .header("Content-Length", contents.len().to_string())
            .body(Full::from(contents))
            .unwrap();

        Ok(response)
    }else {
        let response = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::from("文件不存在"))
            .unwrap();
        Ok(response)
    }
}

async fn handle_request(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, String> {
    match (req.uri().path(), req.method().as_str()) {
        (p,_) if p.contains("/download") => {return download_with_resume(p).await}
        (_) => {}
    }
    let file = std::fs::File::open(Path::new("route_response.json")).map_err(|e| e.to_string())?;
    let json: Vec<RouteResponse> = serde_json::from_reader(file).map_err(|e| e.to_string())?;
    let scheme = &req.uri().scheme();
    //  println!("req.uri():{:?}", &req.uri(),);
    println!("req.uri().path():{:?}  scheme: {:?}", &req.uri().path(), scheme);
    println!("req.uri().query():{:?}  Request.body: {:?}", &req.uri().query(), req.body());
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
    let addr = SocketAddr::from(([0, 0, 0, 0], 8082));
    let listener = TcpListener::bind(addr).map_err(|e| e.to_string()).await?;
    loop {
        let (stream, _) = listener.accept().map_err(|e| e.to_string()).await?;
        let io = TokioIo::new(stream);
        tokio::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(handle_request))
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

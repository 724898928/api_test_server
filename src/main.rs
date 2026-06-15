use bytes::Bytes;
use futures_util::{SinkExt, StreamExt, TryFutureExt, TryStreamExt};
use http_body_util::Full;
use hyper::service::service_fn;
use hyper::{
    body::{Body, Incoming},
    server::conn::http1,
    HeaderMap, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use regex::Regex;
use serde::Deserializer;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::error::Error;
use std::future;
use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
mod mode;
use mode::*;
use tokio::join;
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

fn regex_url(url: &str, re_url: &str) -> bool {
    let mut re_url = re_url.replace("*", "[^/]+");
    re_url = format!("^{}$", re_url);
    let regex = Regex::new(re_url.as_str()).unwrap();
    let url = url.split("?").next().unwrap();
    let res = regex.find(url).is_some();
    //println!("find:{}, regex_url: {} , url: {}",res, re_url, url);
    res
}

async fn download_with_resume(path: &str) -> Result<Response<Full<Bytes>>> {
    let mut respones = Response::new(Full::new("Invalid path".into()));
    if path.contains("..") || path.contains("~") {
        return Ok(respones);
    }
    // 提取文件名
    let mut filename = path.trim_start_matches("/download/").to_string();
    filename = format!("example/{}", filename);
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
            .header(
                "Content-Disposition",
                format!("attachment; filename=\"{}\"", filename),
            )
            .header("Content-Length", contents.len().to_string())
            .body(Full::from(contents))
            .unwrap();

        Ok(response)
    } else {
        let response = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::from("文件不存在"))
            .unwrap();
        Ok(response)
    }
}

async fn handle_request(req: Request<Incoming>) -> Result<Response<Full<Bytes>>> {
    match (req.uri().path(), req.method().as_str()) {
        (p, _) if p.contains("/download") => return download_with_resume(p).await,
        _ => {}
    }
    let file = std::fs::File::open(Path::new("route_response.json")).map_err(|e| e.to_string())?;
    let json: Vec<RouteResponse> = serde_json::from_reader(file).map_err(|e| e.to_string())?;
    let scheme = &req.uri().scheme();
    //  println!("req.uri():{:?}", &req.uri(),);
    println!(
        "req.uri().path():{:?}  scheme: {:?}",
        &req.uri().path(),
        scheme
    );
    println!(
        "req.uri().query():{:?}  Request.body: {:?}",
        &req.uri().query(),
        req.body()
    );
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

pub type Result<T> = std::result::Result<T, String>;

#[tokio::main()]
async fn main() -> Result<()> {
    join!(make_http_server(), make_socket_server());
    // let url = "/api/v2/firmware/uploadauthorize/log.txt?serial=2205H9HD9990&requestId=550e8400-e29b-41d4-a716-446655440000";
    // let res = regex_url(url, "/api/v2/firmware/uploadauthorize/*.txt");
    // println!("res.url():{:?}", &res);

    Ok(())
}

pub async fn create_tcp_server<F, Fut>(addr: &str, f: F) -> Result<()>
where
    F: Fn(TokioIo<TcpStream>) -> Fut + Send + Sync + 'static + Clone,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    let listener = TcpListener::bind(addr).await.map_err(|e| e.to_string())?;
    println!("🚀 异步服务器已启动，监听地址: {}", addr);
    loop {
        let (stream, _addr) = listener.accept().await.map_err(|e| e.to_string())?;
        let io = TokioIo::new(stream);
        let f_clone = f.clone();
        tokio::spawn(async move {
            f_clone(io).await;
        });
    }
}
async fn make_http_server() -> Result<()> {
    let addr = "0.0.0.0:8082";
    create_tcp_server(addr, |mut io| async move {
        if let Err(err) = http1::Builder::new()
            .serve_connection(io, service_fn(handle_request))
            .await
        {
            println!("Error serving connection: {:?}", err);
        }
        Ok(())
    })
    .await
}

async fn make_socket_server() -> Result<()> {
    let addr = "0.0.0.0:8080";
    create_tcp_server(addr, move |mut io| async move {
        let addr = io
            .inner()
            .peer_addr()
            .expect("connected streams should have a peer address");
        println!("Peer address: {}", addr);

        let mut ws_stream = tokio_tungstenite::accept_async(io.into_inner())
            .await
            .expect("Error during the websocket handshake occurred");

        //  println!("New WebSocket connection: {}", addr);

        while let Some(msg) = ws_stream.next().await {
            let msg = msg.map_err(|e| e.to_string())?;
            if msg.is_text() || msg.is_binary() {
                let command = String::from_utf8_lossy(msg.into_data().as_ref()).to_string();
                println!("📨 收到指令 [{}]: {}", addr, &command);
                let response = parse_json2str(&command).await?;
                println!("response: {}", response.to_string());
                ws_stream
                    .send(response)
                    .await
                    .map_err(|e| e.to_string())?;
                // if response.is_string() {
                //     ws_stream.send(Message::from(json_to_bytes(&response)?)).await.map_err(|e| e.to_string())?;
                // }else if response.is_object() || response.is_number() {
                //     ws_stream.send(Message::binary(json_to_bytes(&response)?)).await.map_err(|e| e.to_string())?;
                // }
            }
        }
        Ok(())
    })
    .await?;
    Ok(())
}


async fn parse_json2str(command: &str) -> Result<Message> {
    let file = std::fs::File::open(Path::new("socket_response.json")).map_err(|e| e.to_string())?;
    let json: HashMap<String, HashMap<String, Value>> =
        serde_json::from_reader(file).map_err(|e| e.to_string())?;
    if let Some(val) = json.get(command) {
        // ✅ 正确序列化 JSON → Vec<u8>
        if val.get("info_type").map(|v| v.eq(&0_i8)).unwrap() {
            println!("info_type = {}", val.get("info_type").unwrap().to_string());
            let rest = match val.get("value") {
                Some(val) if val.eq(&1) => [0x01],
                Some(val) if val.eq(&2) => [0x02],
                Some(val) if val.eq(&3) => [0x03],
                _ => [0x00]
            };
            println!("rest = {:x?}", rest);
            return Ok(Message::Binary(Bytes::from(rest.to_vec())));
        } else {
            return Ok(Message::Text(
                Utf8Bytes::from(val.get("value").unwrap().as_str().unwrap_or("")),
            ));
        }
    }
    Ok(Message::text(json!(null).to_string()))
}


async fn parse_socket_val(command: &str) -> Result<Value> {
    let file = std::fs::File::open(Path::new("socket_response.json")).map_err(|e| e.to_string())?;
    let json: HashMap<String, HashMap<String, Value>> =
        serde_json::from_reader(file).map_err(|e| e.to_string())?;
    let val = json
        .get(command)
        .unwrap_or(&HashMap::new())
        .get("value")
        .cloned()
        .unwrap_or(json!(null));
    Ok(val)
}

async fn parse_socket_json(command: &str) -> Result<Bytes> {
    let file = std::fs::File::open(Path::new("socket_response.json")).map_err(|e| e.to_string())?;
    let json: HashMap<String, HashMap<String, Value>> = serde_json::from_reader(file).map_err(|e| e.to_string())?;
    let val = json
        .get(command)
        .unwrap_or(&HashMap::new())
        .get("value")
        .cloned()
        .unwrap_or(json!(null));
    json_to_bytes(&val)
}


fn json_to_bytes(val: &Value) -> Result<Bytes> {
    let vec = serde_json::to_vec(val).map_err(|e| e.to_string())?;
    Ok(Bytes::from(vec))
}

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt, TryFutureExt};
use http_body_util::Full;
use hyper::service::service_fn;
use hyper::{
    body::Incoming,
    server::conn::http1,
    HeaderMap, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

mod mode;
use mode::*;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone)]
struct SocketResponseEntry {
    value: Value,
    info_type: i64,
}

#[derive(Default)]
struct AppState {
    socket_responses: HashMap<String, SocketResponseEntry>,
    route_responses: Vec<RouteResponse>,
    socket_cache_time: Option<Instant>,
    route_cache_time: Option<Instant>,
}

type SharedState = Arc<RwLock<AppState>>;

const CACHE_DURATION: Duration = Duration::from_secs(30);

fn regex_url(url: &str, pattern: &str) -> bool {
    let pattern = pattern.replace('*', "[^/]+");
    let regex = Regex::new(&format!("^{}$", pattern)).unwrap();
    let url = url.split('?').next().unwrap();
    regex.is_match(url)
}

fn is_path_safe(path: &str) -> bool {
    !path.contains("..")
        && !path.contains('~')
        && !path.contains('\0')
        && !path.contains("//")
}

async fn load_socket_responses() -> Result<HashMap<String, SocketResponseEntry>> {
    let file = std::fs::File::open(Path::new("socket_response.json")).map_err(|e| e.to_string())?;
    let json: HashMap<String, HashMap<String, Value>> =
        serde_json::from_reader(file).map_err(|e| e.to_string())?;
    
    let mut result = HashMap::new();
    for (key, val) in json {
        let value = val.get("value").cloned().unwrap_or(json!(null));
        let info_type = val.get("info_type").and_then(|v| v.as_i64()).unwrap_or(1);
        result.insert(key, SocketResponseEntry { value, info_type });
    }
    Ok(result)
}

async fn load_route_responses() -> Result<Vec<RouteResponse>> {
    let file = std::fs::File::open(Path::new("route_response.json")).map_err(|e| e.to_string())?;
    let json: Vec<RouteResponse> = serde_json::from_reader(file).map_err(|e| e.to_string())?;
    Ok(json)
}

async fn get_socket_response(state: &SharedState, command: &str) -> Result<Option<SocketResponseEntry>> {
    {
        let s = state.read().await;
        if let Some(cache_time) = s.socket_cache_time {
            if cache_time.elapsed() < CACHE_DURATION {
                return Ok(s.socket_responses.get(command).cloned());
            }
        }
    }
    
    let responses = load_socket_responses().await?;
    {
        let mut s = state.write().await;
        s.socket_responses = responses.clone();
        s.socket_cache_time = Some(Instant::now());
    }
    Ok(responses.get(command).cloned())
}

async fn get_route_responses(state: &SharedState) -> Result<Vec<RouteResponse>> {
    {
        let s = state.read().await;
        if let Some(cache_time) = s.route_cache_time {
            if cache_time.elapsed() < CACHE_DURATION {
                return Ok(s.route_responses.clone());
            }
        }
    }
    
    let responses = load_route_responses().await?;
    {
        let mut s = state.write().await;
        s.route_responses = responses.clone();
        s.route_cache_time = Some(Instant::now());
    }
    Ok(responses)
}

fn json_to_bytes(val: &Value) -> Result<Bytes> {
    let vec = serde_json::to_vec(val).map_err(|e| e.to_string())?;
    Ok(Bytes::from(vec))
}

async fn parse_json2str(command: &str, state: &SharedState) -> Result<Message> {
    let entry = get_socket_response(state, command).await?;
    
    if let Some(entry) = entry {
        match entry.info_type {
            0 => {
                let rest = match entry.value.as_i64() {
                    Some(1) => [0x01],
                    Some(2) => [0x02],
                    Some(3) => [0x03],
                    _ => [0x00]
                };
                Ok(Message::Binary(Bytes::from(rest.to_vec())))
            }
            2 => {
                Ok(Message::Binary(json_to_bytes(&entry.value)?))
            }
            _ => {
                let text = entry.value.as_str().unwrap_or("");
                Ok(Message::Text(Utf8Bytes::from(text)))
            }
        }
    } else {
        Ok(Message::text(json!(null).to_string()))
    }
}

async fn download_with_resume(path: &str) -> Result<Response<Full<Bytes>>> {
    if !is_path_safe(path) {
        return Ok(Response::new(Full::new("Invalid path".into())));
    }
    
    let filename = path.trim_start_matches("/download/").to_string();
    let file_path_str = format!("example/{}", filename);
    let file_path = Path::new(&file_path_str);
    
    if !file_path.exists() {
        return Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::from("文件不存在"))
            .unwrap());
    }
    
    let mut file = File::open(file_path)
        .map_err(|err| err.to_string())
        .await?;
    
    let mut contents = Vec::new();
    file.read_to_end(&mut contents).await.map_err(|e| e.to_string())?;
    
    let mime_type = mime_guess::from_path(file_path)
        .first_or_octet_stream()
        .to_string();
    
    Ok(Response::builder()
        .header("Content-Type", mime_type)
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        )
        .header("Content-Length", contents.len().to_string())
        .body(Full::from(contents))
        .unwrap())
}

async fn handle_request(
    req: Request<Incoming>,
    state: SharedState,
) -> Result<Response<Full<Bytes>>> {
    let path = req.uri().path();
    
    if path.contains("/download") {
        return download_with_resume(path).await;
    }
    
    let route_responses = get_route_responses(&state).await?;
    
    println!(
        "req.uri().path(): {:?}  scheme: {:?}",
        path,
        req.uri().scheme()
    );
    println!(
        "req.uri().query(): {:?}  Request.body: {:?}",
        req.uri().query(),
        req.body()
    );
    
    let res = route_responses
        .iter()
        .find(|e| {
            regex_url(path, &e.url) && e.method.eq_ignore_ascii_case(req.method().as_str())
        })
        .ok_or_else(|| "no route response found".to_string())?;
    
    println!("response: {:?}", &res.response);
    
    let mut response = Response::new(Full::new(
        serde_json::to_vec(&res.response).unwrap().into(),
    ));
    response.headers_mut().extend(req.headers().clone());
    
    if let Some(map) = &res.headers {
        if !map.is_empty() {
            let headers: HeaderMap = map.try_into().expect("valid headers");
            response.headers_mut().extend(headers);
        }
    }
    
    Ok(response)
}

#[tokio::main()]
async fn main() -> Result<()> {
    let state: SharedState = Arc::new(RwLock::new(AppState::default()));
    
    let state_http = state.clone();
    let state_socket = state.clone();
    
    let (http_result, socket_result) = tokio::join!(
        make_http_server(state_http),
        make_socket_server(state_socket)
    );
    
    if let Err(e) = http_result {
        eprintln!("HTTP server error: {}", e);
    }
    if let Err(e) = socket_result {
        eprintln!("Socket server error: {}", e);
    }
    
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
            let _ = f_clone(io).await;
        });
    }
}

async fn make_http_server(state: SharedState) -> Result<()> {
    let addr = "0.0.0.0:8082";
    create_tcp_server(addr, move |io| {
        let state = state.clone();
        async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(move |req| handle_request(req, state.clone())))
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
            Ok(())
        }
    })
    .await
}

async fn make_socket_server(state: SharedState) -> Result<()> {
    let addr = "0.0.0.0:8080";
    create_tcp_server(addr, move |io| {
        let state = state.clone();
        async move {
            let addr = io
                .inner()
                .peer_addr()
                .expect("connected streams should have a peer address");
            println!("Peer address: {}", addr);

            let mut ws_stream = tokio_tungstenite::accept_async(io.into_inner())
                .await
                .expect("Error during the websocket handshake occurred");

            while let Some(msg) = ws_stream.next().await {
                let msg = msg.map_err(|e| e.to_string())?;
                if msg.is_text() || msg.is_binary() {
                    let command = String::from_utf8_lossy(msg.into_data().as_ref()).to_string();
                    println!("📨 收到指令 [{}]: {}", addr, &command);
                    let response = parse_json2str(&command, &state).await?;
                    println!("response: {}", response.to_string());
                    ws_stream
                        .send(response)
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        }
    })
    .await?;
    Ok(())
}

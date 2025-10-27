// ws_client.rs - Rust version of WebSocket latency measurement client
// Measures: BN->User, CPU (deserialization), MSK (Kafka send), Total latency
// Using sonic-rs for high-performance SIMD JSON parsing

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::util::Timeout;
use serde::Deserialize;
use sonic_rs::{pointer, JsonValueTrait};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

// Global running flag
static RUNNING: AtomicBool = AtomicBool::new(true);
static SEQ_NO: AtomicU64 = AtomicU64::new(1);

// Binance aggTrade message structure
#[derive(Debug, Deserialize)]
struct BinanceAggTrade {
    #[serde(rename = "e")]
    event_type: Option<String>,
    #[serde(rename = "E")]
    event_time: Option<u64>, // milliseconds
    #[serde(rename = "s")]
    symbol: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BinanceCombinedStream {
    stream: Option<String>,
    data: BinanceAggTrade,
}

#[derive(Debug, Deserialize)]
struct BinanceSymbol {
    symbol: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct BinanceExchangeInfo {
    symbols: Vec<BinanceSymbol>,
}

// Latency measurement record
#[derive(Debug, Clone)]
struct LatencyRecord {
    seq_no: u64,
    bn_to_user_ns: u64,   // BN->User: from Binance send time to user receive
    cpu_deser_ns: u64,    // CPU: deserialization time
    msk_send_ns: u64,     // MSK: Kafka send time
    total_ns: u64,        // Total latency
    symbol: String,
    tcp_seq: u32, // For compatibility (not used in Rust version)
}

impl LatencyRecord {
    fn to_log_line(&self) -> String {
        format!(
            "seq={} symbol={} BN->User={}ns CPU={}ns MSK={}ns Total={}ns\n",
            self.seq_no,
            self.symbol,
            self.bn_to_user_ns,
            self.cpu_deser_ns,
            self.msk_send_ns,
            self.total_ns
        )
    }
}

// Async log writer
struct AsyncLogWriter {
    tx: mpsc::UnboundedSender<String>,
}

impl AsyncLogWriter {
    fn new(filename: String) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();

        // Spawn background writer thread
        tokio::spawn(async move {
            let mut file = match OpenOptions::new()
                .create(true)
                .append(true)
                .open(&filename)
                .await
            {
                Ok(f) => f,
                Err(e) => {
                    error!("Failed to open log file {}: {}", filename, e);
                    return;
                }
            };

            let mut write_count = 0u64;
            let mut buffer = Vec::with_capacity(65536);

            while let Some(line) = rx.recv().await {
                buffer.extend_from_slice(line.as_bytes());
                write_count += 1;

                // Flush every 500 writes
                if write_count % 500 == 0 {
                    if let Err(e) = file.write_all(&buffer).await {
                        error!("Failed to write to log file: {}", e);
                    }
                    if let Err(e) = file.flush().await {
                        error!("Failed to flush log file: {}", e);
                    }
                    buffer.clear();
                }
            }

            // Final flush
            if !buffer.is_empty() {
                let _ = file.write_all(&buffer).await;
                let _ = file.flush().await;
            }
        });

        AsyncLogWriter { tx }
    }

    fn write(&self, line: String) {
        if let Err(e) = self.tx.send(line) {
            error!("Failed to send log line: {}", e);
        }
    }
}

// Fetch all USDT trading symbols from Binance Futures
async fn fetch_all_symbols() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let url = "https://fapi.binance.com/fapi/v1/exchangeInfo";
    info!("[REST] Fetching all trading symbols from {}", url);

    let response = reqwest::get(url).await?;
    let exchange_info: BinanceExchangeInfo = response.json().await?;

    let symbols: Vec<String> = exchange_info
        .symbols
        .into_iter()
        .filter(|s| s.status == "TRADING" && s.symbol.ends_with("USDT"))
        .map(|s| s.symbol.to_lowercase())
        .collect();

    info!("[REST] Fetched {} trading symbols", symbols.len());
    Ok(symbols)
}

// Build subscription message for multiple symbols
fn build_subscribe_message(symbols: &[String]) -> String {
    let streams: Vec<String> = symbols
        .iter()
        .map(|s| format!("\"{}@aggTrade\"", s))
        .collect();

    format!(
        r#"{{"method":"SUBSCRIBE","params":[{}],"id":1}}"#,
        streams.join(",")
    )
}

// Parse event time from message using sonic-rs for fast parsing
fn parse_event_time(msg: &str) -> Option<u64> {
    // Use sonic-rs get API for fast field extraction
    // Try combined stream: data.E
    let data_e_path = pointer!["data", "E"];
    if let Ok(value) = sonic_rs::get(msg, &data_e_path) {
        if let Some(time) = value.as_u64() {
            return Some(time);
        }
    }
    
    // Try direct format: E
    let e_path = pointer!["E"];
    if let Ok(value) = sonic_rs::get(msg, &e_path) {
        if let Some(time) = value.as_u64() {
            return Some(time);
        }
    }

    None
}

// Extract symbol from message using sonic-rs
fn extract_symbol(msg: &str) -> String {
    // Try combined stream: data.s
    let data_s_path = pointer!["data", "s"];
    if let Ok(value) = sonic_rs::get(msg, &data_s_path) {
        if let Some(symbol) = value.as_str() {
            return symbol.to_string();
        }
    }
    
    // Try direct format: s
    let s_path = pointer!["s"];
    if let Ok(value) = sonic_rs::get(msg, &s_path) {
        if let Some(symbol) = value.as_str() {
            return symbol.to_string();
        }
    }

    "UNKNOWN".to_string()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    // Setup signal handler
    let running = Arc::new(RUNNING);
    let running_clone = running.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Failed to listen for ctrl+c");
        info!("[signal] Caught SIGINT (Ctrl+C), exiting...");
        running_clone.store(false, Ordering::SeqCst);
    });

    // Parse command-line arguments
    let args: Vec<String> = std::env::args().collect();
    let enable_log_file = args.iter().any(|a| a == "-wlogs");
    let uri = args
        .iter()
        .find(|a| a.starts_with("wss://") || a.starts_with("ws://"))
        .cloned()
        .unwrap_or_else(|| "wss://fstream.binance.com:443/ws/stream".to_string());

    // Create async log writer if enabled
    let async_writer = if enable_log_file {
        let log_filename = format!("latency_{}.log", Utc::now().timestamp());
        info!("[wlogs] Async latency log writer enabled: {}", log_filename);
        Some(AsyncLogWriter::new(log_filename))
    } else {
        None
    };

    // Fetch all symbols
    let symbols = match fetch_all_symbols().await {
        Ok(s) if !s.is_empty() => s,
        _ => {
            warn!("[REST] Failed to fetch symbols, using defaults");
            vec!["btcusdt".to_string(), "ethusdt".to_string()]
        }
    };

    // Limit to 205 symbols (Binance connection limit)
    let symbols = if symbols.len() > 205 {
        warn!(
            "[WS] Symbol count {} exceeds limit 205, truncating",
            symbols.len()
        );
        symbols[..205].to_vec()
    } else {
        symbols
    };

    let subscribe_msg = build_subscribe_message(&symbols);
    info!(
        "[WS] Subscribe message length: {} bytes, symbols count: {}",
        subscribe_msg.len(),
        symbols.len()
    );

    // Initialize Kafka producer
    let producer: Arc<FutureProducer> = Arc::new(
        ClientConfig::new()
            .set("bootstrap.servers", "localhost:9092")
            .set("message.timeout.ms", "5000")
            .set("queue.buffering.max.ms", "0") // Low latency
            .create()
            .expect("Failed to create Kafka producer")
    );
    info!("[Kafka] Producer initialized (localhost:9092)");

    // Connect to WebSocket
    info!("[WS] Connecting to {}", uri);
    let (ws_stream, _) = connect_async(&uri).await?;
    info!("[WS] Connected successfully");

    let (mut write, mut read) = ws_stream.split();

    // Send subscription message
    info!("[WS] Sending subscription...");
    write
        .send(WsMessage::Text(subscribe_msg))
        .await?;
    info!("[WS] Subscription sent");

    // Message processing loop
    let producer_clone = producer.clone();
    let async_writer_clone = async_writer;

    while RUNNING.load(Ordering::SeqCst) {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        process_message(
                            &text,
                            &producer_clone,
                            &async_writer_clone,
                        ).await;
                    }
                    Some(Ok(WsMessage::Binary(_))) => {
                        // Skip binary messages
                    }
                    Some(Ok(WsMessage::Close(_))) => {
                        info!("[WS] Connection closed by server");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("[WS] Error receiving message: {}", e);
                        break;
                    }
                    None => {
                        info!("[WS] Stream ended");
                        break;
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(1)) => {
                // Prevent busy-waiting
            }
        }
    }

    // Cleanup
    info!("[WS] Shutting down...");
    // Kafka producer will be dropped automatically
    info!("[WS] Shutdown complete");

    Ok(())
}

async fn process_message(
    text: &str,
    producer: &Arc<FutureProducer>,
    async_writer: &Option<AsyncLogWriter>,
) {
    // Timestamp 1: User receive time (both steady and system)
    let ts_userspace_start = Instant::now();
    let ts_userspace_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    // Extract Binance event time (E field)
    let bn_send_time_ms = match parse_event_time(text) {
        Some(t) => t,
        None => {
            // Skip non-trade messages (e.g., subscription confirmations)
            return;
        }
    };
    let bn_send_time_ns = bn_send_time_ms * 1_000_000;

    // Timestamp 2: After deserialization
    let ts_after_deser = Instant::now();
    let cpu_deser_ns = ts_after_deser.duration_since(ts_userspace_start).as_nanos() as u64;

    // Extract symbol
    let symbol = extract_symbol(text);

    // Send to Kafka and measure time
    let ts_before_msk = Instant::now();
    
    let msk_msg = format!(
        r#"{{"bornTimestamp":{},"data":{{"symbol":"{}","ts":{},"exchange":"WSCC-BN"}}}}"#,
        ts_userspace_epoch / 1_000_000, // Convert to milliseconds
        symbol,
        bn_send_time_ns
    );

    let topic = "binance_latency";
    let key = symbol.as_bytes();
    let mut msk_send_ns = 0u64;

    let record = FutureRecord::to(topic)
        .key(key)
        .payload(&msk_msg);
    
    match producer.send(record, Timeout::After(Duration::from_millis(100))).await {
        Ok(_) => {
            let ts_after_msk = Instant::now();
            msk_send_ns = ts_after_msk.duration_since(ts_before_msk).as_nanos() as u64;
        }
        Err(_) => {
            // Silently skip if Kafka is not available - MSK latency will be 0
        }
    }

    // Calculate BN->User latency
    // Need to align timestamps: steady clock (Instant) vs system clock (SystemTime)
    // Use the user receive epoch time and subtract BN send time
    let bn_to_user_ns = if ts_userspace_epoch > bn_send_time_ns {
        ts_userspace_epoch - bn_send_time_ns
    } else {
        0
    };

    // Calculate total latency
    let total_ns = bn_to_user_ns + cpu_deser_ns + msk_send_ns;

    // Create latency record
    let seq_no = SEQ_NO.fetch_add(1, Ordering::SeqCst);
    let record = LatencyRecord {
        seq_no,
        bn_to_user_ns,
        cpu_deser_ns,
        msk_send_ns,
        total_ns,
        symbol: symbol.clone(),
        tcp_seq: 0,
    };

    // Log to console
    info!(
        "[delay] seq={} symbol={} BN->User={}ns CPU={}ns MSK={}ns Total={}ns",
        record.seq_no,
        record.symbol,
        record.bn_to_user_ns,
        record.cpu_deser_ns,
        record.msk_send_ns,
        record.total_ns
    );

    // Write to async log file if enabled
    if let Some(writer) = async_writer {
        writer.write(record.to_log_line());
    }
}

# Binance WebSocket 延迟测量 - Rust 版本

Rust 版本的 WebSocket 延迟测量客户端，用于与 C++ 版本进行性能比较。

## 核心功能

- 连接币安期货 WebSocket，订阅所有 USDT 交易对（最多 205 个）
- 使用 sonic-rs (SIMD) 进行高性能 JSON 解析
- 测量四个延迟指标：**BN->User**、**CPU**、**MSK**、**Total**
- 异步日志写入，Kafka 消息队列集成

## 前置条件

### 1. 安装 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 2. 安装 Kafka（可选）

需要测量 MSK 延迟时才需要安装 Kafka（默认端口 9092）。

```bash
# 使用 Docker 快速启动 Kafka
docker run -d --name kafka -p 9092:9092 \
  -e KAFKA_ENABLE_KRAFT=yes \
  -e KAFKA_CFG_PROCESS_ROLES=broker,controller \
  -e KAFKA_CFG_CONTROLLER_LISTENER_NAMES=CONTROLLER \
  -e KAFKA_CFG_LISTENERS=PLAINTEXT://:9092,CONTROLLER://:9093 \
  -e KAFKA_CFG_LISTENER_SECURITY_PROTOCOL_MAP=CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT \
  -e KAFKA_CFG_ADVERTISED_LISTENERS=PLAINTEXT://localhost:9092 \
  -e KAFKA_BROKER_ID=1 \
  -e KAFKA_CFG_CONTROLLER_QUORUM_VOTERS=1@localhost:9093 \
  bitnami/kafka:latest
```

## 快速开始

```bash
# 构建
cd /root/bn-test-r
cargo build --release

# 运行（基本）
cargo run --release

# 运行（带日志）
cargo run --release -- -wlogs

# 或使用脚本
./start.sh -wlogs
```

## 输出格式

```
[delay] seq=1 symbol=BTCUSDT BN->User=45123456ns CPU=125000ns MSK=3456789ns Total=48705245ns
```


## 技术栈对比

| 组件 | Rust | C++ |
|------|------|-----|
| JSON 解析 | sonic-rs (SIMD) | simdjson (SIMD) |
| 消息队列 | Kafka (rdkafka) | ZeroMQ |
| 异步运行时 | Tokio | Boost.Asio |

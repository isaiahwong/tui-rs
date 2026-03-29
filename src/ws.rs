use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::tungstenite::Message::Text;

use crate::types::{BookSnapshot, Candle, Message};

#[derive(Deserialize, Debug)]
struct KlineUpdate {
    pub k: RawCandle,
}

#[derive(Deserialize, Debug, Clone)]
struct Depth {
    pub b: Vec<(String, String)>,
    pub a: Vec<(String, String)>,
}

impl From<Depth> for BookSnapshot {
    fn from(depth: Depth) -> Self {
        let parse = |raw: Vec<(String, String)>| {
            raw.into_iter()
                .filter_map(|(p, q)| {
                    let p = p.parse::<f64>().ok()?;
                    let q = q.parse::<f64>().ok()?;
                    Some((p, q))
                })
                .collect()
        };

        BookSnapshot {
            bids: parse(depth.b),
            asks: parse(depth.a),
        }
    }
}

#[derive(Deserialize, Debug)]
struct RawCandle {
    pub t: i64,
    pub o: String,
    pub h: String,
    pub l: String,
    pub c: String,
    pub v: String,
    pub x: bool,
}

impl From<RawCandle> for Candle {
    fn from(raw: RawCandle) -> Self {
        Self {
            timestamp: raw.t,
            open: raw.o.parse().unwrap_or(0.0),
            high: raw.h.parse().unwrap_or(0.0),
            low: raw.l.parse().unwrap_or(0.0),
            close: raw.c.parse().unwrap_or(0.0),
            volume: raw.v.parse().unwrap_or(0.0),
            is_closed: raw.x,
        }
    }
}

#[derive(Deserialize)]
struct StreamMessage {
    #[allow(dead_code)]
    pub stream: String,
    pub data: StreamData,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StreamData {
    Depth(Depth),
    Kline(KlineUpdate),
}

pub async fn run(symbol: String, tx: Sender<anyhow::Result<Message>>) -> anyhow::Result<()> {
    let symbol = symbol.to_lowercase();

    // Fetch initial candles snapshot
    let candles = fetch_klines(&symbol).await?;
    tx.send(Ok(Message::CandleSnapshot(candles))).await?;

    let url = format!(
        "wss://fstream.binance.com/stream?streams={}@depth20@100ms/{}@kline_1m",
        symbol, symbol
    );

    let connect = tokio_tungstenite::connect_async(&url).await?;
    let (ws, _) = connect;
    let (_, mut read) = ws.split();

    // Mux
    while let Some(m) = read.next().await.transpose()? {
        let Text(msg) = m else { continue };

        match serde_json::from_str::<StreamMessage>(&msg)? {
            StreamMessage {
                data: StreamData::Depth(depth),
                ..
            } => {
                tx.send(Ok(Message::BookSnapshot(depth.into()))).await?;
            }
            StreamMessage {
                data: StreamData::Kline(update),
                ..
            } => {
                tx.send(Ok(Message::Candle(update.k.into()))).await?;
            }
        }
    }

    Ok(())
}

async fn fetch_klines(symbol: &str) -> anyhow::Result<Vec<Candle>> {
    let url = format!(
        "https://fapi.binance.com/fapi/v1/klines?symbol={}&interval=1m&limit=100",
        symbol.to_uppercase()
    );

    let resp = reqwest::get(url)
        .await?
        .json::<Vec<Vec<serde_json::Value>>>()
        .await?;

    let candles = resp
        .into_iter()
        .filter_map(|v| {
            if v.len() >= 6 {
                Some(Candle {
                    timestamp: v[0].as_i64()?,
                    open: v[1].as_str()?.parse().ok()?,
                    high: v[2].as_str()?.parse().ok()?,
                    low: v[3].as_str()?.parse().ok()?,
                    close: v[4].as_str()?.parse().ok()?,
                    volume: v[5].as_str()?.parse().ok()?,
                    is_closed: true,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(candles)
}

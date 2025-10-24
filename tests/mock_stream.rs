#![cfg(feature = "mock")]

use booklid_rust::{AngleDevice, OpenOptions, open_with};
use futures_util::StreamExt;
use tokio::time::{Duration, sleep, timeout};

#[tokio::test(flavor = "current_thread")]
async fn open_with_mock_returns_and_latest_updates() {
    let dev = open_with(OpenOptions::new(60.0).allow_mock(true))
        .await
        .expect("open mock");
    // latest should become Some within ~1s
    let mut found = false;
    for _ in 0..20 {
        if dev.latest().is_some() {
            found = true;
            break;
        }
        sleep(Duration::from_millis(50)).await;
    }
    assert!(found, "latest() did not become Some in time");
}

#[tokio::test(flavor = "current_thread")]
async fn subscribe_yields_items_quickly() {
    let dev = open_with(OpenOptions::new(60.0).allow_mock(true))
        .await
        .expect("open mock");
    let mut s = dev.subscribe();
    // Ensure we get at least one item within 500ms
    let item = timeout(Duration::from_millis(500), s.next())
        .await
        .expect("no timeout");
    assert!(item.is_some(), "stream ended unexpectedly");
}

#[tokio::test(flavor = "current_thread")]
async fn smoothing_reduces_jitter() {
    let dev = open_with(OpenOptions::new(120.0).allow_mock(true))
        .await
        .expect("open mock");
    // Collect window with low smoothing (snappy)
    dev.set_smoothing(0.0);
    let mut s = dev.subscribe();
    let mut deltas_raw = Vec::new();
    let mut prev = s.next().await.unwrap();
    for _ in 0..120 {
        let next = s.next().await.unwrap();
        deltas_raw.push((next.angle_deg - prev.angle_deg).abs());
        prev = next;
    }
    // Collect window with high smoothing (laggy)
    dev.set_smoothing(0.9);
    let mut deltas_smooth = Vec::new();
    let mut prev = s.next().await.unwrap();
    for _ in 0..120 {
        let next = s.next().await.unwrap();
        deltas_smooth.push((next.angle_deg - prev.angle_deg).abs());
        prev = next;
    }
    // Compare average absolute delta (proxy for jitter)
    let avg_raw: f32 = deltas_raw.iter().copied().sum::<f32>() / deltas_raw.len() as f32;
    let avg_smooth: f32 = deltas_smooth.iter().copied().sum::<f32>() / deltas_smooth.len() as f32;
    assert!(
        avg_smooth < avg_raw,
        "smoothing did not reduce jitter: smooth={} raw={}",
        avg_smooth,
        avg_raw
    );
}

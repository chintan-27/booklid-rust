#![cfg(feature = "mock")]

use booklid_rust::{OpenConfig, open_with_config};

use futures_util::StreamExt;
use tokio::time::{Duration, sleep, timeout};

#[tokio::test(flavor = "current_thread")]
async fn open_with_mock_returns_and_latest_updates() {
    let dev = open_with_config(OpenConfig::new(60.0).allow_mock(true))
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
    let dev = open_with_config(OpenConfig::new(60.0).allow_mock(true))
        .await
        .expect("open mock");
    let mut s = dev.subscribe();
    // Ensure we get at least one item within 750ms (a touch more lenient)
    let item = timeout(Duration::from_millis(750), s.next())
        .await
        .expect("no timeout");
    assert!(item.is_some(), "stream ended unexpectedly");
}
#[tokio::test(flavor = "current_thread")]
async fn smoothing_reduces_jitter() {
    // use futures_util::StreamExt;

    let dev = open_with_config(OpenConfig::new(60.0).allow_mock(true))
        .await
        .expect("open mock");

    // RAW (no smoothing)
    dev.set_smoothing(1.0);
    let mut s1 = dev.subscribe();
    warmup(&mut s1, 64).await;
    let var_raw = variance_over(&mut s1, 512).await;

    // SMOOTHED (heavy smoothing)
    dev.set_smoothing(0.05);
    let mut s2 = dev.subscribe();
    warmup(&mut s2, 64).await;
    let var_smooth = variance_over(&mut s2, 512).await;

    // Expect lower variance with smoothing; keep tolerance modest to avoid flakiness
    assert!(
        var_smooth < var_raw * 0.95,
        "smoothing did not reduce variance: smooth={var_smooth} raw={var_raw}"
    );
}

async fn warmup<S>(s: &mut S, n: usize)
where
    S: futures_util::Stream<Item = booklid_rust::AngleSample> + Unpin,
{
    for _ in 0..n {
        let _ = s.next().await;
    }
}

async fn variance_over<S>(s: &mut S, n: usize) -> f32
where
    S: futures_util::Stream<Item = booklid_rust::AngleSample> + Unpin,
{
    let mut vals = Vec::with_capacity(n);
    while vals.len() < n {
        if let Some(x) = s.next().await {
            vals.push(x.angle_deg);
        } else {
            break;
        }
    }
    let m = vals.iter().copied().sum::<f32>() / (vals.len().max(1) as f32);
    vals.iter()
        .map(|v| {
            let d = *v - m;
            d * d
        })
        .sum::<f32>()
        / (vals.len().max(1) as f32)
}

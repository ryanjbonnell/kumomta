use crate::kumod::{DaemonWithMaildir, MailGenParams};
use kumo_api_types::SuspendV1Response;
use kumo_log_types::RecordType::{Delivery, Reception};
use std::time::Duration;

#[tokio::test]
async fn rebind_timerwheel() -> anyhow::Result<()> {
    rebind_impl("TimerWheel").await
}
#[tokio::test]
async fn rebind_skiplist() -> anyhow::Result<()> {
    rebind_impl("SkipList").await
}

async fn rebind_impl(strategy: &str) -> anyhow::Result<()> {
    let mut daemon =
        DaemonWithMaildir::start_with_env(vec![("KUMOD_QUEUE_STRATEGY", strategy)]).await?;
    let mut client = daemon.smtp_client().await?;

    let status: SuspendV1Response = daemon
        .kcli_json(["suspend", "--domain", "example.com", "--reason", "testing"])
        .await?;
    println!("kcli status: {status:?}");

    let response = MailGenParams {
        recip: Some("allow@example.com"),
        ..Default::default()
    }
    .send(&mut client)
    .await?;
    eprintln!("{response:?}");
    anyhow::ensure!(response.code == 250);

    daemon
        .wait_for_source_summary(
            |summary| summary.get(&Reception).copied().unwrap_or(0) > 0,
            Duration::from_secs(5),
        )
        .await;

    daemon
        .kcli([
            "rebind",
            "--domain",
            "example.com",
            "--reason",
            "testing",
            "--data",
            "{\"queue\":\"rebound.com\"}",
        ])
        .await?;

    daemon
        .wait_for_source_summary(
            |summary| summary.get(&Delivery).copied().unwrap_or(0) > 0,
            Duration::from_secs(5),
        )
        .await;

    daemon.stop_both().await?;
    let delivery_summary = daemon.dump_logs()?;
    k9::snapshot!(
        delivery_summary,
        "
DeliverySummary {
    source_counts: {
        Reception: 1,
        Delivery: 1,
        AdminRebind: 1,
    },
    sink_counts: {
        Reception: 1,
        Delivery: 1,
    },
}
"
    );
    Ok(())
}

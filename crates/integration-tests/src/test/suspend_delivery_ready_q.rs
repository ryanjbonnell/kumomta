use crate::kumod::{DaemonWithMaildir, MailGenParams};
use kumo_api_types::SuspendV1Response;
use kumo_log_types::RecordType::{Delivery, Reception};
use std::time::Duration;

#[tokio::test]
async fn suspend_delivery_ready_q() -> anyhow::Result<()> {
    let mut daemon = DaemonWithMaildir::start().await?;
    let mut client = daemon.smtp_client().await?;

    let status: SuspendV1Response = daemon
        .kcli_json([
            "suspend-ready-q",
            "--name",
            "unspecified->mx_list:localhost@smtp_client",
            "--reason",
            "testing",
        ])
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

    // Allow a little bit of time for a delivery to go through
    // if for some reason suspension is broken
    daemon
        .wait_for_source_summary(
            |summary| summary.get(&Delivery).copied().unwrap_or(0) > 0,
            Duration::from_secs(5),
        )
        .await;

    daemon
        .wait_for_source_summary(
            |summary| summary.get(&Reception).copied().unwrap_or(0) > 0,
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
        TransientFailure: 1,
    },
    sink_counts: {},
}
"
    );
    k9::snapshot!(
        daemon.source.accounting_stats()?,
        "
AccountingStats {
    received: 1,
    delivered: 0,
}
"
    );
    Ok(())
}

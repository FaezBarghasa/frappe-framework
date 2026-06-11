use std::time::Duration;
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WebhookJob {
    pub url: String,
    pub payload: serde_json::Value,
    pub max_retries: u8,
    pub attempt: u8,
}

pub struct WebhookWorker;

impl WebhookWorker {
    /// Spawns a background task to execute a WebhookJob with exponential backoff.
    pub fn spawn_job(job: WebhookJob) {
        tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default();

            let backoff_delays = [
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(8),
                Duration::from_secs(16),
                Duration::from_secs(32),
            ];

            let mut current_job = job;

            loop {
                log::info!(
                    "Sending webhook request to {}, attempt {}/{}",
                    current_job.url,
                    current_job.attempt + 1,
                    current_job.max_retries
                );

                let res = client.post(&current_job.url)
                    .json(&current_job.payload)
                    .send()
                    .await;

                match res {
                    Ok(resp) if resp.status().is_success() => {
                        log::info!("Webhook request to {} succeeded", current_job.url);
                        break;
                    }
                    Ok(resp) => {
                        log::warn!(
                            "Webhook request to {} returned status code: {}",
                            current_job.url,
                            resp.status()
                        );
                    }
                    Err(e) => {
                        log::error!("Webhook request to {} failed: {:?}", current_job.url, e);
                    }
                }

                current_job.attempt += 1;
                if current_job.attempt >= current_job.max_retries {
                    log::error!("Webhook request to {} reached maximum retries. Aborting.", current_job.url);
                    break;
                }

                let delay_index = (current_job.attempt as usize - 1).min(backoff_delays.len() - 1);
                let delay = backoff_delays[delay_index];
                log::info!("Retrying webhook to {} in {:?}", current_job.url, delay);
                tokio::time::sleep(delay).await;
            }
        });
    }
}

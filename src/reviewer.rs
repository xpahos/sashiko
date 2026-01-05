use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tracing::{info, error, warn};
use crate::db::Database;
use crate::settings::Settings;
use serde_json::json;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;

pub struct Reviewer {
    db: Arc<Database>,
    settings: Settings,
    semaphore: Arc<Semaphore>,
}

impl Reviewer {
    pub fn new(db: Arc<Database>, settings: Settings) -> Self {
        let concurrency = settings.review.concurrency;
        Self {
            db,
            settings,
            semaphore: Arc::new(Semaphore::new(concurrency)),
        }
    }

    pub async fn start(&self) {
        info!("Starting Reviewer service with concurrency limit: {}", self.settings.review.concurrency);
        
        // Cleanup worktree directory on startup
        let worktree_dir = PathBuf::from(&self.settings.review.worktree_dir);
        if worktree_dir.exists() {
            info!("Cleaning up previous worktree directory: {:?}", worktree_dir);
            if let Err(e) = std::fs::remove_dir_all(&worktree_dir) {
                error!("Failed to cleanup worktree directory: {}", e);
            }
        }
        if let Err(e) = std::fs::create_dir_all(&worktree_dir) {
            error!("Failed to create worktree directory: {}", e);
        }

        // Reset any patchsets stuck in 'Reviewing' state from previous run
        match self.db.reset_reviewing_status().await {
            Ok(count) => {
                if count > 0 {
                    info!("Recovered {} interrupted reviews (reset to Pending)", count);
                }
            },
            Err(e) => error!("Failed to reset reviewing status: {}", e),
        }

        loop {
            match self.process_pending_patchsets().await {
                Ok(_) => {},
                Err(e) => error!("Error in reviewer loop: {}", e),
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        }
    }

    async fn process_pending_patchsets(&self) -> Result<()> {
        // Fetch pending patchsets
        let patchsets = self.db.get_pending_patchsets(10).await?;

        if patchsets.is_empty() {
            return Ok(());
        }

        info!("Found {} pending patchsets for review", patchsets.len());

        for patchset in patchsets {
            let permit = self.semaphore.clone().acquire_owned().await?;
            let db = self.db.clone();
            let settings = self.settings.clone();
            let patchset_id = patchset.id;
            let subject = patchset.subject.clone().unwrap_or("Unknown".to_string());

            tokio::spawn(async move {
                let _permit = permit;
                
                info!("Starting review for patchset {}", patchset_id);
                
                if let Err(e) = db.update_patchset_status(patchset_id, "Reviewing").await {
                    error!("Failed to update status to Reviewing for {}: {}", patchset_id, e);
                    return;
                }

                // Fetch patches/diffs to pass to tool
                let diffs = match db.get_patch_diffs(patchset_id).await {
                    Ok(d) => d,
                    Err(e) => {
                        error!("Failed to fetch diffs for {}: {}", patchset_id, e);
                        let _ = db.update_patchset_status(patchset_id, "Failed").await;
                        return;
                    }
                };

                let patches_json: Vec<_> = diffs.into_iter().map(|(idx, diff)| {
                    json!({ "index": idx, "diff": diff })
                }).collect();

                let input_payload = json!({
                    "id": patchset_id,
                    "subject": subject,
                    "patches": patches_json
                });

                match run_review_tool(&input_payload, &settings).await {
                    Ok(status) => {
                        info!("Review finished for {}: {}", patchset_id, status);
                        if let Err(e) = db.update_patchset_status(patchset_id, &status).await {
                            error!("Failed to update status for {}: {}", patchset_id, e);
                        }
                    },
                    Err(e) => {
                        error!("Review failed for {}: {}", patchset_id, e);
                        if let Err(e) = db.update_patchset_status(patchset_id, "Failed").await {
                            error!("Failed to update status for {}: {}", patchset_id, e);
                        }
                    }
                }
            });
        }

        Ok(())
    }
}

async fn run_review_tool(input_payload: &serde_json::Value, settings: &Settings) -> Result<String> {
    let exe_path = std::env::current_exe()?;
    let bin_dir = exe_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let review_bin = bin_dir.join("review");

    let mut cmd = if review_bin.exists() {
        Command::new(review_bin)
    } else {
        warn!("Could not find review binary at {:?}, falling back to cargo run", review_bin);
        let mut c = Command::new("cargo");
        c.args(["run", "--bin", "review", "--"]);
        c
    };

    cmd.args([
        "--json", // Use JSON mode
        "--baseline", "HEAD", 
        "--worktree-dir", &settings.review.worktree_dir,
    ]);

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;

    // Write input JSON to stdin
    if let Some(mut stdin) = child.stdin.take() {
        let input_str = serde_json::to_string(input_payload)?;
        stdin.write_all(input_str.as_bytes()).await?;
    }

    let output = child.wait_with_output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("Review tool failed (exit code {:?}): {}", output.status.code(), stderr);
        return Ok("Failed".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Parse JSON output
    let json: serde_json::Value = serde_json::from_str(&stdout)?;
    
    // Check if all patches applied
    let patches = json["patches"].as_array().ok_or_else(|| anyhow::anyhow!("Invalid output JSON: no patches"))?;
    let all_applied = patches.iter().all(|p| p["status"] == "applied");

    if all_applied {
        Ok("Applied".to_string())
    } else {
        for p in patches.iter().filter(|p| p["status"] == "failed").take(3) {
            warn!("Patch {} failed: {}", p["index"], p["stderr"].as_str().unwrap_or(""));
        }
        Ok("Failed".to_string())
    }
}
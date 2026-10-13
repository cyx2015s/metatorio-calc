//! Diagnostic collection, deliberately separate from the feasibility regressions.
use super::*;
use std::{
    collections::BTreeSet,
    io::Write,
    path::{Path, PathBuf},
};

fn snapshots(directory: &Path) -> BTreeSet<PathBuf> {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect()
}

#[tokio::test]
#[ignore = "captures historical failing requests; diagnostic outcomes are not pass-rate assertions"]
async fn experiment_capture_failure_requests() {
    let _permit = GATE.acquire().await.unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let request_path = std::env::var_os("LP_REQUESTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("crates/metatorio-shell/src/mcp/auto_plan_assets_tests/fixtures/lp-failure-requests.jsonl"));
    let directory = PathBuf::from(
        std::env::var_os("METATORIO_LP_CAPTURE_DIR").expect("METATORIO_LP_CAPTURE_DIR is required"),
    );
    std::fs::create_dir_all(&directory).unwrap();
    let offset = std::env::var("LP_REQUEST_OFFSET")
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(0);
    let limit = std::env::var("LP_REQUEST_LIMIT")
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(usize::MAX);
    let requests: Vec<Value> = std::fs::read_to_string(request_path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| {
            row["error_summary"].as_str().unwrap().contains("校验")
                || row["error_summary"].as_str().unwrap().contains("无界")
        })
        .skip(offset)
        .take(limit)
        .collect();
    assert!(!requests.is_empty(), "no selected diagnostic requests");
    let output_path = directory.join(format!("request-outcomes-{offset}.jsonl"));
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)
        .unwrap();
    let mut counts = std::collections::BTreeMap::new();
    for (index, request) in requests.iter().enumerate() {
        let asset = request["asset"].as_str().unwrap();
        assert!(matches!(asset, BASE | MODDED));
        eprintln!(
            "[LP-REQUEST] {} {asset}/{}",
            offset + index,
            request["item"]
        );
        let before = snapshots(&directory);
        let mcp = handler(asset);
        let start_time = Instant::now();
        let ids = start(&mcp, request["arguments"].clone()).await;
        let state = finish(&mcp, &ids).await;
        let status = state["auto_plan"]["status"].as_str().unwrap();
        let captured: Vec<_> = snapshots(&directory).difference(&before).cloned().collect();
        let error = state["auto_plan"]["error"]
            .as_str()
            .map(|s| s.split('；').next().unwrap());
        let record = json!({"request_index":offset+index,"request":request,"status":status,"error_summary":error,
            "elapsed_ms":start_time.elapsed().as_millis(),"snapshot_paths":captured,
            "report":state["auto_plan"]["result"]["report"]});
        writeln!(output, "{record}").unwrap();
        output.flush().unwrap();
        assert!(!captured.is_empty(), "LP capture did not occur");
        assert!(matches!(status, "done" | "failed"));
        *counts.entry(status.to_string()).or_insert(0usize) += 1;
        if status == "done" {
            assert_solved(&mcp, &ids, &state);
        } else {
            assert!(
                state["auto_plan"]["error"]
                    .as_str()
                    .unwrap()
                    .starts_with("自动规划无解："),
                "infrastructure failure: {state}"
            );
        }
    }
    eprintln!("[LP-REQUEST-OUTCOMES] {counts:?}; diagnostic collection only");
}

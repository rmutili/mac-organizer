//! Splits a scan into batches, asks Claude for a plan per batch, and merges and
//! validates the results.
//!
//! One request over all names designs the folder set first; every batch is
//! then restricted to those folders (schema enum) and batches run concurrently.

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Mutex;

use futures::stream::{self, StreamExt, TryStreamExt};
use serde::Serialize;
use serde_json::Value;

use super::plan::{Plan, RawPlan, validate_folder_name, validate_plan};
use super::request::{build_folders_request, build_request};
use crate::scanner::types::FileItem;

pub const BATCH_SIZE: usize = 150;
const CONCURRENT_BATCHES: usize = 4;

pub const ALLOWED_MODELS: [&str; 3] = ["claude-haiku-5-5", "claude-sonnet-5-5", "claude-opus-5-5"];
/// Haiku is fast and light on usage; Opus is the "thorough" option in the UI.
pub const DEFAULT_MODEL: &str = "claude-haiku-5-5";

/// How many file names from the in-flight batches the UI shows.
pub const PREVIEW_NAMES: usize = 5;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassifyProgress {
    /// Batches finished so far.
    pub done: usize,
    pub total: usize,
    /// A few names from the batches Claude is working on right now.
    pub sorting: Vec<String>,
    /// How many items those in-flight batches hold in total.
    pub sorting_count: usize,
}

/// Tracks finished and in-flight batches and reports every change.
struct Tracker<'a, P> {
    total: usize,
    on_progress: P,
    state: Mutex<(usize, BTreeMap<usize, &'a [FileItem]>)>,
}

impl<'a, P: Fn(&ClassifyProgress)> Tracker<'a, P> {
    fn new(total: usize, on_progress: P) -> Self {
        Self {
            total,
            on_progress,
            state: Mutex::new((0, BTreeMap::new())),
        }
    }

    fn update(&self, change: impl FnOnce(&mut (usize, BTreeMap<usize, &'a [FileItem]>))) {
        // Progress is display-only, so a poisoned lock just keeps the last state.
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut state);
        let (done, active) = &*state;
        (self.on_progress)(&ClassifyProgress {
            done: *done,
            total: self.total,
            sorting: active
                .values()
                .flat_map(|batch| batch.iter())
                .take(PREVIEW_NAMES)
                .map(|item| item.name.clone())
                .collect(),
            sorting_count: active.values().map(|batch| batch.len()).sum(),
        });
    }

    fn start(&self, index: usize, batch: &'a [FileItem]) {
        self.update(|(_, active)| {
            active.insert(index, batch);
        });
    }

    fn finish(&self, index: usize) {
        self.update(|(done, active)| {
            active.remove(&index);
            *done += 1;
        });
    }
}

/// `send` turns one Messages-API-shaped request body into a raw plan. It's
/// injected so the same batching works for the API and the Claude Code CLI,
/// and so tests don't touch the network.
///
/// `on_progress` is called once at the start and whenever a batch starts or
/// finishes.
pub async fn classify<F, Fut, P>(
    items: &[FileItem],
    model: &str,
    now: i64,
    send: F,
    on_progress: P,
) -> Result<Plan, String>
where
    F: Fn(Value) -> Fut,
    Fut: Future<Output = Result<RawPlan, String>>,
    P: Fn(&ClassifyProgress),
{
    if items.is_empty() {
        return Ok(validate_plan(RawPlan::default(), items));
    }
    let batches: Vec<&[FileItem]> = items.chunks(BATCH_SIZE).collect();
    // Step 0 designs the folders; steps 1..=n are the batches.
    let tracker = Tracker::new(batches.len() + 1, on_progress);
    tracker.update(|_| {});

    tracker.start(0, &[]);
    let proposed = send(build_folders_request(model, items)).await?;
    tracker.finish(0);
    let folders = usable_folder_names(&proposed);
    if folders.is_empty() {
        return Err("Claude didn't propose any usable folders. Try again.".into());
    }

    // Futures are lazy: each batch is marked as started only when `buffered`
    // actually polls it, so the progress reflects what's really in flight.
    let (tracker, send, folders_ref) = (&tracker, &send, &folders);
    let pending: Vec<_> = batches
        .iter()
        .enumerate()
        .map(|(offset, &batch)| {
            let index = offset + 1;
            let body = build_request(model, batch, folders_ref, now);
            async move {
                tracker.start(index, batch);
                let plan = send(body).await?;
                tracker.finish(index);
                Ok::<_, String>(plan)
            }
        })
        .collect();
    let batch_plans: Vec<RawPlan> = stream::iter(pending)
        .buffered(CONCURRENT_BATCHES)
        .try_collect()
        .await?;

    let base = RawPlan {
        folders: proposed.folders,
        ..RawPlan::default()
    };
    let merged = batch_plans.into_iter().fold(base, RawPlan::merge);
    Ok(validate_plan(merged, items))
}

/// Valid folder names, de-duplicated case-insensitively, in proposal order.
fn usable_folder_names(proposed: &RawPlan) -> Vec<String> {
    proposed
        .folders
        .iter()
        .filter_map(|f| validate_folder_name(&f.name).ok())
        .fold(Vec::new(), |mut names: Vec<String>, name| {
            if !names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                names.push(name);
            }
            names
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::request::FOLDERS_TOOL_NAME;
    use crate::claude::response::parse_plan;
    use crate::scanner::types::Category;
    use serde_json::json;

    fn items(n: u32) -> Vec<FileItem> {
        (0..n)
            .map(|id| FileItem {
                id,
                name: format!("file{id}.pdf"),
                extension: "pdf".into(),
                category: Category::Document,
                is_dir: false,
                size_bytes: 1,
                file_count: 1,
                modified_at: 0,
                accessed_at: 0,
                duplicate_group: None,
                sensitive: false,
            })
            .collect()
    }

    /// Fake API: the folder request gets "Docs" and "Old"; batches put every
    /// item into "Docs".
    fn fake_response(request: &Value) -> Value {
        if request["tools"][0]["name"] == FOLDERS_TOOL_NAME {
            return json!({
                "stop_reason": "tool_use",
                "content": [{ "type": "tool_use", "name": FOLDERS_TOOL_NAME, "input": {
                    "folders": [
                        { "name": "Docs", "description": "" },
                        { "name": "../bad", "description": "" },
                        { "name": "Old", "description": "" }
                    ]
                }}]
            });
        }
        let payload: Value =
            serde_json::from_str(request["messages"][0]["content"].as_str().unwrap()).unwrap();
        let assignments: Vec<Value> = payload["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| json!({ "item_id": i["id"], "folder": "Docs", "reason": "pdf" }))
            .collect();
        json!({
            "stop_reason": "tool_use",
            "content": [{ "type": "tool_use", "name": "propose_plan", "input": {
                "assignments": assignments,
                "deletion_candidates": []
            }}]
        })
    }

    async fn fake_send(req: Value) -> Result<RawPlan, String> {
        Ok(parse_plan(&fake_response(&req)).unwrap())
    }

    #[tokio::test]
    async fn designs_folders_first_then_restricts_every_batch_to_them() {
        let all = items(BATCH_SIZE as u32 * 2 + 5);
        let seen: Mutex<Vec<Value>> = Mutex::new(Vec::new());

        let plan = classify(
            &all,
            DEFAULT_MODEL,
            0,
            |req| {
                seen.lock().unwrap().push(req.clone());
                fake_send(req)
            },
            |_: &ClassifyProgress| {},
        )
        .await
        .unwrap();

        let requests = seen.into_inner().unwrap();
        assert_eq!(requests.len(), 4);
        assert_eq!(requests[0]["tools"][0]["name"], FOLDERS_TOOL_NAME);
        for batch in &requests[1..] {
            let allowed = &batch["tools"][0]["input_schema"]["properties"]["assignments"]["items"]
                ["properties"]["folder"]["enum"];
            assert_eq!(allowed, &json!(["Docs", "Old"]));
        }
        assert_eq!(plan.moves.len(), all.len());
        let names: Vec<&str> = plan.folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["Docs", "Old"]);
    }

    #[tokio::test]
    async fn fails_when_no_usable_folders_are_proposed() {
        let result = classify(
            &items(3),
            DEFAULT_MODEL,
            0,
            |_req| async { Ok(RawPlan::default()) },
            |_: &ClassifyProgress| {},
        )
        .await;

        assert!(result.unwrap_err().contains("folders"));
    }

    #[tokio::test]
    async fn reports_progress_with_files_being_sorted() {
        let all = items(BATCH_SIZE as u32 * 2 + 5);
        let events: Mutex<Vec<ClassifyProgress>> = Mutex::new(Vec::new());

        classify(&all, DEFAULT_MODEL, 0, fake_send, |p: &ClassifyProgress| {
            events.lock().unwrap().push(p.clone())
        })
        .await
        .unwrap();

        let events = events.into_inner().unwrap();
        let first = &events[0];
        assert_eq!((first.done, first.total, first.sorting_count), (0, 4, 0));
        // events[1] is folder design starting; events[2] is it finishing.
        assert_eq!(events[2].done, 1, "folder design counts as the first step");
        let batch = events.iter().find(|e| e.sorting_count > 0).unwrap();
        assert_eq!(batch.sorting[0], "file0.pdf");
        assert!(batch.sorting.len() <= PREVIEW_NAMES);
        let last = events.last().unwrap();
        assert_eq!((last.done, last.total, last.sorting_count), (4, 4, 0));
        assert!(events.windows(2).all(|w| w[0].done <= w[1].done));
    }

    #[test]
    fn defaults_to_the_fast_model() {
        assert_eq!(DEFAULT_MODEL, "claude-haiku-5-5");
    }

    #[tokio::test]
    async fn any_failed_batch_fails_the_whole_run() {
        let result = classify(
            &items(BATCH_SIZE as u32 + 1),
            DEFAULT_MODEL,
            0,
            |_req| async { Err::<RawPlan, String>("boom".into()) },
            |_: &ClassifyProgress| {},
        )
        .await;

        assert_eq!(result, Err("boom".to_string()));
    }

    #[tokio::test]
    async fn empty_scan_makes_no_requests() {
        let plan = classify(
            &[],
            DEFAULT_MODEL,
            0,
            |_req| async { panic!("should not be called") },
            |_: &ClassifyProgress| panic!("no batches, no progress"),
        )
        .await
        .unwrap();

        assert!(plan.moves.is_empty());
    }
}

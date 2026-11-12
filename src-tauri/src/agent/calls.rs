use std::future::Future;
use std::sync::atomic::Ordering;

use super::{AgentRun, tools};

type CallResults = Vec<Option<anyhow::Result<tools::ToolResult>>>;

/// Phase 0 calls run first, one at a time in call order; phase 1 calls then run together; higher phases run
/// last, one at a time, lower phase first and in call order within a phase.
pub(super) async fn run_calls<J, F, Fut>(
    jobs: Vec<J>,
    run: &AgentRun,
    phase: impl Fn(&J) -> usize,
    exec: F,
    mut done: impl FnMut(usize, &anyhow::Result<tools::ToolResult>),
) -> (CallResults, bool)
where
    F: Fn(J) -> Fut,
    Fut: Future<Output = anyhow::Result<tools::ToolResult>> + Send + 'static,
{
    let mut results: CallResults = jobs.iter().map(|_| None).collect();
    if run.stop.load(Ordering::SeqCst) {
        return (results, true);
    }
    let mut tagged: Vec<(usize, usize, J)> = jobs
        .into_iter()
        .enumerate()
        .map(|(i, j)| (phase(&j), i, j))
        .collect();
    tagged.sort_by_key(|(p, i, _)| (*p, *i));
    let (first, rest): (Vec<_>, Vec<_>) = tagged.into_iter().partition(|(p, _, _)| *p == 0);
    let (together, last): (Vec<_>, Vec<_>) = rest.into_iter().partition(|(p, _, _)| *p == 1);
    if run_in_order(first, run, &exec, &mut results, &mut done).await {
        return (results, true);
    }
    let mut set = tokio::task::JoinSet::new();
    for (_, i, job) in together {
        let task = exec(job);
        set.spawn(async move { (i, task.await) });
    }
    loop {
        let stop = run.stop_notify.notified();
        tokio::pin!(stop);
        stop.as_mut().enable();
        if run.stop.load(Ordering::SeqCst) {
            set.abort_all();
            return (results, true);
        }
        tokio::select! {
            joined = set.join_next() => match joined {
                None => break,
                Some(Ok((i, result))) => {
                    done(i, &result);
                    results[i] = Some(result);
                }
                Some(Err(e)) => log::warn!("tool task ended abnormally: {e}"),
            },
            () = &mut stop => {}
        }
    }
    let stopped = run_in_order(last, run, &exec, &mut results, &mut done).await;
    (results, stopped)
}

/// Returns true when the user stopped the turn.
async fn run_in_order<J, F, Fut>(
    jobs: Vec<(usize, usize, J)>,
    run: &AgentRun,
    exec: &F,
    results: &mut CallResults,
    done: &mut impl FnMut(usize, &anyhow::Result<tools::ToolResult>),
) -> bool
where
    F: Fn(J) -> Fut,
    Fut: Future<Output = anyhow::Result<tools::ToolResult>>,
{
    for (_, i, job) in jobs {
        let stop = run.stop_notify.notified();
        tokio::pin!(stop);
        stop.as_mut().enable();
        if run.stop.load(Ordering::SeqCst) {
            return true;
        }
        tokio::select! {
            result = exec(job) => {
                done(i, &result);
                results[i] = Some(result);
            }
            () = &mut stop => return true,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    async fn waited(ms: u64) -> anyhow::Result<tools::ToolResult> {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        Ok(tools::ToolResult {
            text: ms.to_string(),
            images: vec![],
            cached: false,
        })
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn tool_calls_run_at_the_same_time() {
        let run = Arc::new(AgentRun::new());
        let started = std::time::Instant::now();
        let mut finished = Vec::new();
        let (results, stopped) = run_calls(
            vec![300u64, 100, 200],
            &run,
            |_| 1,
            waited,
            |i, _| finished.push(i),
        )
        .await;
        let elapsed = started.elapsed().as_millis();
        assert!(!stopped);
        assert!(elapsed < 450, "calls ran one after another: {elapsed} ms");
        assert_eq!(finished, vec![1, 2, 0]);
        let texts: Vec<String> = results
            .into_iter()
            .map(|r| r.expect("finished").expect("ok").text)
            .collect();
        assert_eq!(texts, vec!["300", "100", "200"]);

        let stopper = run.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            stopper.request_stop();
        });
        let started = std::time::Instant::now();
        let (results, stopped) = run_calls(vec![2000u64, 2000], &run, |_| 1, waited, |_, _| {}).await;
        assert!(stopped);
        assert!(started.elapsed().as_millis() < 1000);
        assert!(results.iter().all(Option::is_none));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn ordered_calls_finish_before_the_rest_start() {
        let run = Arc::new(AgentRun::new());
        let started = std::time::Instant::now();
        let mut finished = Vec::new();
        let (results, stopped) = run_calls(
            vec![101u64, 20, 103, 20],
            &run,
            |ms| usize::from(ms % 2 == 0),
            waited,
            |i, _| finished.push((i, started.elapsed().as_millis())),
        )
        .await;
        assert!(!stopped);
        assert!(results.iter().all(Option::is_some));
        let order: Vec<usize> = finished.iter().map(|f| f.0).collect();
        assert_eq!(order[..2], [0, 2], "{finished:?}");
        let ordered_done = finished[1].1;
        assert!(ordered_done >= 204, "ordered calls overlapped: {finished:?}");
        assert!(
            finished[2..].iter().all(|f| f.1 >= ordered_done + 15),
            "{finished:?}"
        );
        assert!(
            started.elapsed().as_millis() < 400,
            "the rest did not run together"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn layout_tools_run_last_in_their_own_order() {
        let run = Arc::new(AgentRun::new());
        let jobs = [
            "fit_box",
            "add_layer",
            "arrange_row",
            "set_plan",
            "add_layer",
            "arrange_row",
        ];
        let mut finished = Vec::new();
        let (results, stopped) = run_calls(
            jobs.iter().enumerate().map(|(i, n)| (i, *n)).collect(),
            &run,
            |(_, name): &(usize, &str)| tools::phase(name),
            |(i, name): (usize, &str)| async move {
                waited(match (i, name) {
                    (1, _) => 40,
                    (_, "add_layer") => 10,
                    _ => 5,
                })
                .await
            },
            |i, _| finished.push(i),
        )
        .await;
        assert!(!stopped && results.iter().all(Option::is_some));
        assert_eq!(finished, [3, 4, 1, 2, 5, 0], "{finished:?}");
    }
}

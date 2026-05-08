use std::time::Duration;

use agentflow_server::{InMemoryWorkerProtocol, WorkerId};
use agentflow_worker::{WorkerConfig, WorkerRuntime};

#[tokio::main]
async fn main() {
  if let Err(e) = run().await {
    eprintln!("agentflow-worker: {e}");
    std::process::exit(1);
  }
}

async fn run() -> Result<(), String> {
  let args = Args::parse(std::env::args().skip(1))?;
  let worker_id = WorkerId::new(args.worker_id).map_err(|e| e.to_string())?;
  let mut config = WorkerConfig::new(worker_id, args.control_plane);
  config.poll_interval = args.poll_interval;
  config.heartbeat_interval = args.heartbeat_interval;

  if config.control_plane != "memory://local" {
    return Err(format!(
      "control plane '{}' is not available yet; use memory://local for local smoke tests",
      config.control_plane
    ));
  }

  let runtime = WorkerRuntime::new(InMemoryWorkerProtocol::new(), config);
  if args.once {
    let _ = runtime.run_once().await.map_err(|e| e.to_string())?;
  } else {
    runtime.run_forever().await.map_err(|e| e.to_string())?;
  }
  Ok(())
}

#[derive(Debug)]
struct Args {
  worker_id: String,
  control_plane: String,
  once: bool,
  poll_interval: Duration,
  heartbeat_interval: Duration,
}

impl Args {
  fn parse<I>(mut args: I) -> Result<Self, String>
  where
    I: Iterator<Item = String>,
  {
    let mut parsed = Self {
      worker_id: std::env::var("AGENTFLOW_WORKER_ID").unwrap_or_else(|_| "worker-local".into()),
      control_plane: std::env::var("AGENTFLOW_CONTROL_PLANE")
        .unwrap_or_else(|_| "memory://local".into()),
      once: false,
      poll_interval: Duration::from_millis(250),
      heartbeat_interval: Duration::from_secs(5),
    };

    while let Some(arg) = args.next() {
      match arg.as_str() {
        "--worker-id" => {
          parsed.worker_id = args
            .next()
            .ok_or_else(|| "--worker-id requires a value".to_string())?;
        }
        "--control-plane" => {
          parsed.control_plane = args
            .next()
            .ok_or_else(|| "--control-plane requires a value".to_string())?;
        }
        "--once" => parsed.once = true,
        "--poll-ms" => {
          let value = args
            .next()
            .ok_or_else(|| "--poll-ms requires a value".to_string())?;
          let millis = value
            .parse::<u64>()
            .map_err(|_| "--poll-ms must be an integer".to_string())?;
          parsed.poll_interval = Duration::from_millis(millis);
        }
        "--heartbeat-ms" => {
          let value = args
            .next()
            .ok_or_else(|| "--heartbeat-ms requires a value".to_string())?;
          let millis = value
            .parse::<u64>()
            .map_err(|_| "--heartbeat-ms must be an integer".to_string())?;
          parsed.heartbeat_interval = Duration::from_millis(millis);
        }
        "--help" | "-h" => return Err(help()),
        other => return Err(format!("unknown argument: {other}\n{}", help())),
      }
    }
    Ok(parsed)
  }
}

fn help() -> String {
  "usage: agentflow-worker [--worker-id ID] [--control-plane memory://local] [--once] [--poll-ms N] [--heartbeat-ms N]".into()
}

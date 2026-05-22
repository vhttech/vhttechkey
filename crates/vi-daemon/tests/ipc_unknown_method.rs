//! Verify that the IPC server returns `Response::Error` when a `set_method`
//! request contains an unrecognised method name.

use std::path::PathBuf;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use vi_daemon::ipc::{Request, Response};

#[tokio::test]
async fn test_ipc_unknown_method_returns_error() {
    let path = PathBuf::from(format!(
        "/tmp/vi-test-ipc-unknown-method-{}.sock",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);

    // Handler that mirrors the validation in main.rs: only "telex", "vni", and
    // "viqr" are accepted; anything else is an error.
    let handler = |req: Request| match req {
        Request::SetMethod { method } => {
            if matches!(method.as_str(), "telex" | "vni" | "viqr") {
                Response::Ok
            } else {
                Response::Error {
                    message: format!("Unknown method: {method}"),
                }
            }
        }
        _ => Response::Ok,
    };

    let server_path = path.clone();
    tokio::spawn(async move {
        vi_daemon::ipc::serve(server_path, handler, Duration::from_secs(30)).await;
    });

    tokio::time::sleep(Duration::from_millis(20)).await;

    let mut stream = UnixStream::connect(&path).await.expect("connect");

    let req_json = serde_json::to_string(&Request::SetMethod {
        method: "bogus".into(),
    })
    .expect("serialize")
        + "\n";
    stream.write_all(req_json.as_bytes()).await.expect("write");

    let (reader, _writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let line = lines.next_line().await.expect("read").expect("line");
    let resp: Response = serde_json::from_str(&line).expect("parse response");

    assert!(
        matches!(resp, Response::Error { .. }),
        "unknown method must produce Response::Error, got: {resp:?}"
    );

    std::fs::remove_file(&path).ok();
}

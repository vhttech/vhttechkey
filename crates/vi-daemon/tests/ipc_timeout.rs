//! Verify that the IPC server closes a connection when a client sends partial
//! JSON (no terminating newline) and the read timeout expires.

use std::path::PathBuf;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use vi_daemon::ipc::{Request, Response};

#[tokio::test]
async fn test_ipc_read_timeout_closes_connection() {
    let path = PathBuf::from(format!(
        "/tmp/vi-test-ipc-timeout-{}.sock",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);

    let server_path = path.clone();
    // Use a 1-second timeout so the test completes quickly.
    tokio::spawn(async move {
        vi_daemon::ipc::serve(
            server_path,
            |_req: Request| Response::Ok,
            Duration::from_secs(1),
        )
        .await;
    });

    tokio::time::sleep(Duration::from_millis(20)).await;

    let mut stream = UnixStream::connect(&path).await.expect("connect");

    // Send partial JSON without a terminating newline — the server will never
    // see a complete line and must time out.
    stream
        .write_all(b"{\"cmd\":\"status\"")
        .await
        .expect("write partial");

    // Hold the write half alive so the client does not send EOF first.
    let (reader, _writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    // The server's 1 s timeout fires; give 4 s total before declaring failure.
    let result = tokio::time::timeout(Duration::from_secs(4), lines.next_line()).await;

    match result {
        // EOF (None) or a read error both indicate the server closed its end.
        Ok(Ok(None)) | Ok(Err(_)) => {}
        Err(_outer_timeout) => {
            panic!("server did not close the connection within 4 s (read-timeout not enforced)")
        }
        Ok(Ok(Some(line))) => panic!("unexpected line from server: {line}"),
    }

    std::fs::remove_file(&path).ok();
}

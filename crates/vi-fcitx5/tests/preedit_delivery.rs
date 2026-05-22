//! Regression: OP PREEDIT responses must be delivered (flushed to the socket)
//! after EACH composing keystroke — before OP COMMIT — preventing preedit
//! updates from being batched and delayed until the word is committed.
//!
//! Uses a real `tokio::net::UnixStream` pair so the test exercises the full
//! write + flush path inside `handle_shim_connection`, not just the in-memory
//! `dispatch_shim_line` string formatter.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use vi_core::{InputMethod, StandardEngine};

// ── Protocol helper ───────────────────────────────────────────────────────────

/// Read one complete shim response: "RESULT <consumed> <num_ops>" header line
/// followed by `num_ops` "OP …" lines.  Returns the full text with embedded
/// newlines, mirroring what `dispatch_shim_line` produces.
async fn read_response(
    lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
) -> String {
    let header = lines
        .next_line()
        .await
        .expect("socket read must not fail")
        .expect("socket must not close unexpectedly");

    let num_ops: usize = header
        .split_ascii_whitespace()
        .nth(2)
        .expect("RESULT line must have three fields")
        .parse()
        .expect("op count must be a number");

    let mut out = header;
    for _ in 0..num_ops {
        let op = lines
            .next_line()
            .await
            .expect("socket read must not fail")
            .expect("socket closed mid-response");
        out.push('\n');
        out.push_str(&op);
    }
    out
}

/// Send a newline-terminated command and read back one complete response.
async fn send_recv(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    cmd: &[u8],
) -> String {
    writer.write_all(cmd).await.expect("write must succeed");
    // flush() is a no-op for UnixStream but documents intent.
    writer.flush().await.expect("flush must succeed");
    read_response(lines).await
}

// ── Test ──────────────────────────────────────────────────────────────────────

/// Type "t","o","o","i"," " via the real socket protocol and assert:
///
/// 1. OP PREEDIT arrives after each of the first four keys (no batching).
/// 2. OP COMMIT arrives after space and contains no OP PREEDIT.
/// 3. Responses are ordered: preedit for "t" arrives before preedit for "to"
///    (verified by the sequential send/recv structure and cursor-count checks).
#[tokio::test]
async fn preedit_delivered_per_keystroke_over_socket() {
    let (client, server) = UnixStream::pair().expect("socket pair must be created");
    let engine = Arc::new(Mutex::new(StandardEngine::new(InputMethod::Telex)));

    // Run the shim handler on the server end of the socket pair.
    tokio::spawn(vi_fcitx5::handle_shim_connection(
        server,
        Arc::clone(&engine),
    ));

    let (read_half, mut write_half) = client.into_split();
    let mut lines = BufReader::new(read_half).lines();

    // ── 't' (keyval 116, no modifiers) ───────────────────────────────────────
    // Expected: RESULT 1 1 / OP PREEDIT 1 <hex "t">
    let r_t = send_recv(&mut write_half, &mut lines, b"KEY 116 0 0\n").await;
    assert!(
        r_t.contains("OP PREEDIT"),
        "'t': expected OP PREEDIT; got {r_t:?}"
    );
    assert!(
        r_t.contains("PREEDIT 1 "),
        "'t': cursor must be 1 (one char); got {r_t:?}"
    );

    // KEYUP 't' resets the repeat-guard so the second 'o' fires the oo→ô rule.
    send_recv(&mut write_half, &mut lines, b"KEYUP 116\n").await;

    // ── 'o' (keyval 111) ─────────────────────────────────────────────────────
    // Expected: RESULT 1 1 / OP PREEDIT 2 <hex "to">
    let r_o = send_recv(&mut write_half, &mut lines, b"KEY 111 0 0\n").await;
    assert!(
        r_o.contains("OP PREEDIT"),
        "'o': expected OP PREEDIT; got {r_o:?}"
    );
    assert!(
        r_o.contains("PREEDIT 2 "),
        "'o': cursor must be 2 (\"to\"); got {r_o:?}"
    );

    // KEYUP 'o' so the second press triggers the oo→ô Telex rule.
    send_recv(&mut write_half, &mut lines, b"KEYUP 111\n").await;

    // ── 'o' (second press, oo→ô rule fires) ──────────────────────────────────
    // Expected: RESULT 1 1 / OP PREEDIT 2 <hex "tô">  (still 2 chars)
    let r_o2 = send_recv(&mut write_half, &mut lines, b"KEY 111 0 0\n").await;
    assert!(
        r_o2.contains("OP PREEDIT"),
        "second 'o': expected OP PREEDIT (oo→ô); got {r_o2:?}"
    );
    assert!(
        r_o2.contains("PREEDIT 2 "),
        "second 'o': cursor must be 2 (\"tô\" = 2 chars); got {r_o2:?}"
    );

    // ── 'i' (keyval 105) ─────────────────────────────────────────────────────
    // Expected: RESULT 1 1 / OP PREEDIT 3 <hex "tôi">
    let r_i = send_recv(&mut write_half, &mut lines, b"KEY 105 0 0\n").await;
    assert!(
        r_i.contains("OP PREEDIT"),
        "'i': expected OP PREEDIT; got {r_i:?}"
    );
    assert!(
        r_i.contains("PREEDIT 3 "),
        "'i': cursor must be 3 (\"tôi\"); got {r_i:?}"
    );

    // ── ' ' (space, keyval 32) ────────────────────────────────────────────────
    // Commits "tôi" via CommitThenPassThrough.
    // Expected: RESULT 1 2 / OP COMMIT <hex "tôi"> / OP FWDKEY 32 0 0
    let r_sp = send_recv(&mut write_half, &mut lines, b"KEY 32 0 0\n").await;
    assert!(
        r_sp.contains("OP COMMIT"),
        "space: expected OP COMMIT; got {r_sp:?}"
    );
    assert!(
        !r_sp.contains("OP PREEDIT"),
        "space: must not emit OP PREEDIT at commit time; got {r_sp:?}"
    );

    // ── Ordering assertion ────────────────────────────────────────────────────
    // The sequential send/recv guarantees that r_t arrived before r_o.
    // We additionally assert the cursor counts grow monotonically up to the
    // commit, confirming no batching occurred.
    assert!(
        r_t.contains("PREEDIT 1 ") && r_o.contains("PREEDIT 2 "),
        "preedit for 't' (cursor 1) must arrive before preedit for 'to' (cursor 2); \
         r_t = {r_t:?}, r_o = {r_o:?}"
    );
    assert!(
        r_o2.contains("PREEDIT 2 ") && r_i.contains("PREEDIT 3 "),
        "preedit cursor must grow from 2 (\"tô\") to 3 (\"tôi\"); \
         r_o2 = {r_o2:?}, r_i = {r_i:?}"
    );
}

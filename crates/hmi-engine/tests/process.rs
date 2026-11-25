//! The real `hmi-engine` binary over stdin/stdout, as the Kotlin client uses it.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn engine() -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_hmi-engine"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

#[test]
fn speaks_json_lines_and_refuses_garbage_without_dying() {
    let mut child = engine();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    let mut ask = |request: &str, stdin: &mut std::process::ChildStdin| {
        writeln!(stdin, "{request}").unwrap();
        stdin.flush().unwrap();
        line.clear();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str::<serde_json::Value>(&line).unwrap()
    };
    let reply = ask(r#"{"type":"hello","id":1,"protocol":1}"#, &mut stdin);
    assert_eq!(reply["code"], "UNSUPPORTED_PROTOCOL");
    let reply = ask(r#"{"type":"start","id":2}"#, &mut stdin);
    assert_eq!(reply["code"], "HELLO_REQUIRED");
    let reply = ask("this is not json", &mut stdin);
    assert_eq!(reply["code"], "BAD_REQUEST");
    let reply = ask(r#"{"type":"hello","id":3,"protocol":3}"#, &mut stdin);
    assert_eq!(reply["type"], "helloOk");
    let reply = ask(r#"{"type":"shutdown","id":4}"#, &mut stdin);
    assert_eq!(reply["type"], "ok");
    assert!(child.wait().unwrap().success());
}

/// The engine must not outlive its client: closing stdin disconnects and exits.
#[test]
fn exits_when_its_client_goes_away() {
    let mut child = engine();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, r#"{{"type":"hello","id":1,"protocol":3}}"#).unwrap();
    writeln!(
        stdin,
        r#"{{"type":"configure","id":2,"config":{{"version":1,"sources":[{{"id":"sim","driver":"loopback","settings":{{}}}}],"points":[]}},"credentials":{{}}}}"#
    )
    .unwrap();
    writeln!(stdin, r#"{{"type":"start","id":3}}"#).unwrap();
    drop(stdin);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "engine kept running without a client"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn await_exit(child: &mut std::process::Child) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if child.try_wait().unwrap().is_some() {
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("engine did not exit");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn output_disconnect_with_input_still_open() {
    let mut child = engine();
    let mut input = child.stdin.take().unwrap();
    drop(child.stdout.take());
    let _ = writeln!(input, r#"{{"type":"hello","id":1,"protocol":3}}"#);
    await_exit(&mut child);
    drop(input);
}

#[test]
fn oversized_unterminated_frame_ends_session() {
    let mut child = engine();
    let mut input = child.stdin.take().unwrap();
    let _ = input.write_all(&vec![b'x'; 1024 * 1024 + 1]);
    await_exit(&mut child);
    drop(input);
}

#[test]
fn unread_output_flood_ends_session() {
    let mut child = engine();
    let mut input = child.stdin.take().unwrap();
    // Keep stdout open but never drain it. The writer must not block indefinitely.
    let feeder = std::thread::spawn(move || {
        for id in 1..100_000 {
            if writeln!(input, r#"{{"type":"hello","id":{id},"protocol":3}}"#).is_err() {
                break;
            }
        }
        input
    });
    await_exit(&mut child);
    drop(feeder.join().unwrap());
}

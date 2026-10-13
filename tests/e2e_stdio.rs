use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use tempfile::TempDir;

fn write_fake_flute2(dir: &TempDir, stdout: &str, exit_code: i32) -> std::path::PathBuf {
    let path = dir.path().join("flute2");
    let script = format!(
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{}'\nexit {}\n",
        stdout.replace('\'', "'\\''"),
        exit_code,
    );
    fs::write(&path, script).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
}

fn jsonrpc(id: u64, method: &str, params: Value) -> String {
    format!(
        "{}\n",
        serde_json::to_string(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .unwrap()
    )
}

fn jsonrpc_notify(method: &str, params: Value) -> String {
    format!(
        "{}\n",
        serde_json::to_string(&json!({"jsonrpc":"2.0","method":method,"params":params})).unwrap()
    )
}

fn read_one_frame(reader: &mut BufReader<impl std::io::Read>) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).expect("read frame");
    serde_json::from_str(line.trim()).expect("parse frame")
}

fn start(fake: &std::path::Path, profile: &str) -> std::process::Child {
    start_with_env(fake, profile, &[("RUST_LOG", "warn")])
}

fn start_with_env(
    fake: &std::path::Path,
    profile: &str,
    env: &[(&str, &str)],
) -> std::process::Child {
    let bin = assert_cmd::cargo::cargo_bin("flute2-mcp");
    let mut cmd = Command::new(bin);
    cmd.env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("FLUTE2_BIN", fake)
        .env("FLUTE2_PROFILE", profile)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.spawn().expect("spawn server")
}

fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .expect("kill -0")
        .success()
}

/// Polls `f` every 50 ms for up to ten seconds.
fn wait_until(mut f: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if f() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    false
}

/// Returns the `result` of `initialize`.
fn handshake(stdin: &mut impl Write, reader: &mut BufReader<impl std::io::Read>) -> Value {
    handshake_with_id(stdin, reader, 1)
}

fn handshake_with_id(
    stdin: &mut impl Write,
    reader: &mut BufReader<impl std::io::Read>,
    id: u64,
) -> Value {
    stdin
        .write_all(
            jsonrpc(
                id,
                "initialize",
                json!({
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": { "name": "e2e", "version": "0" }
                }),
            )
            .as_bytes(),
        )
        .unwrap();
    let init = read_one_frame(reader);
    assert_eq!(init["id"], id);
    assert!(init["result"].is_object());
    stdin
        .write_all(jsonrpc_notify("notifications/initialized", json!({})).as_bytes())
        .unwrap();
    init["result"].clone()
}

fn call_tool(
    stdin: &mut impl Write,
    reader: &mut BufReader<impl std::io::Read>,
    id: u64,
    name: &str,
    arguments: Value,
) -> Value {
    stdin
        .write_all(
            jsonrpc(
                id,
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .as_bytes(),
        )
        .unwrap();
    read_one_frame(reader)
}

/// The tool payload, decoded out of the MCP content envelope.
fn content(frame: &Value) -> Value {
    serde_json::from_str(frame["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn lists_exactly_fifty_three_tools_and_calls_a_read() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute2(&dir, r#"{"object":"ping","data":{"ok":true}}"#, 0);
    let mut child = start(&fake, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    let init = handshake(&mut stdin, &mut reader);
    assert_eq!(init["serverInfo"]["name"], "flute2-mcp");

    stdin
        .write_all(jsonrpc(2, "tools/list", json!({})).as_bytes())
        .unwrap();
    let listed = read_one_frame(&mut reader);
    let tools = listed["result"]["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 53, "expected 53 tools, got {}", tools.len());

    // None of the excluded commands may have leaked in as a tool.
    for absent in [
        "auth_login",
        "auth_logout",
        "auth_switch",
        "auth_token",
        "update",
        "completion",
        "transactions_inspect",
    ] {
        assert!(
            !tools.iter().any(|t| t["name"] == absent),
            "{absent} must not be exposed"
        );
    }

    let called = call_tool(&mut stdin, &mut reader, 3, "ping", json!({}));
    assert_eq!(called["result"]["isError"], false);
    assert_eq!(content(&called)["data"]["ok"], true);

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn every_page_size_schema_bounds_it_to_one_through_a_hundred() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute2(&dir, "{}", 0);
    let mut child = start(&fake, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    handshake(&mut stdin, &mut reader);

    stdin
        .write_all(jsonrpc(2, "tools/list", json!({})).as_bytes())
        .unwrap();
    let listed = read_one_frame(&mut reader);
    let paged: Vec<&Value> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["inputSchema"]["properties"]["page_size"].is_object())
        .collect();
    assert_eq!(paged.len(), 7);
    for t in paged {
        let s = &t["inputSchema"]["properties"]["page_size"];
        assert_eq!(s["minimum"], 1, "{}", t["name"]);
        assert_eq!(s["maximum"], 100, "{}", t["name"]);
    }

    let _ = child.kill();
    let _ = child.wait();
}

/// Money-moving creates must carry their hazard in the description the client
/// shows the model, not only in the readme.
#[test]
fn money_moving_create_descriptions_carry_their_warnings() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute2(&dir, r#"{"object":"ping"}"#, 0);
    let mut child = start(&fake, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    handshake(&mut stdin, &mut reader);

    stdin
        .write_all(jsonrpc(2, "tools/list", json!({})).as_bytes())
        .unwrap();
    let listed = read_one_frame(&mut reader);
    let tools = listed["result"]["tools"].as_array().unwrap();
    let describe = |name: &str| -> String {
        tools
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing from tools/list"))
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or_else(|| panic!("{name} has no description"))
            .to_string()
    };

    // The six writes that take a reference id: all unsafe to retry, and the
    // reference id is the knob that makes a retry safe.
    for name in [
        "transactions_create",
        "transactions_credit",
        "pos_create",
        "pos_reversal",
        "payment_links_create",
        "payment_sessions_create",
    ] {
        let desc = describe(name);
        assert!(
            desc.contains("NOT safe to retry"),
            "{name} must warn that it is not safe to retry: {desc}"
        );
        assert!(
            desc.contains("reference_id"),
            "{name} must name reference_id as the de-duplication knob: {desc}"
        );
    }

    // The writes that move money report the outcome in a status field, because
    // the exit code cannot.
    for (name, status_field) in [
        ("transactions_create", "data.transactionStatus"),
        ("transactions_credit", "data.transactionStatus"),
        ("pos_create", "data.linkedTransaction.transactionStatus"),
        ("pos_reversal", "data.linkedTransaction.transactionStatus"),
        ("pos_get", "data.linkedTransaction.transactionStatus"),
    ] {
        let desc = describe(name);
        assert!(
            desc.contains(status_field),
            "{name} must name {status_field} as the field that says whether money moved: {desc}"
        );
    }
    for name in ["transactions_create", "transactions_credit"] {
        assert!(
            describe(name).contains("DECLINE IS A SUCCESS"),
            "{name} must state that a decline arrives as a success"
        );
    }

    // The fifteen writes whose operation declares `Idempotency-Key` take the
    // key, and no other tool does.
    let keyed: std::collections::BTreeSet<&str> = tools
        .iter()
        .filter(|t| t["inputSchema"]["properties"]["idempotency_key"].is_object())
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        keyed,
        std::collections::BTreeSet::from([
            "payment_links_create",
            "payment_links_share",
            "payment_sessions_cancel",
            "payment_sessions_create",
            "pos_cancel",
            "pos_create",
            "pos_reversal",
            "settlements_close",
            "transactions_ach_hold",
            "transactions_ach_release",
            "transactions_capture",
            "transactions_create",
            "transactions_credit",
            "transactions_reversal",
            "transactions_tip_adjust",
        ])
    );
    for name in &keyed {
        assert!(
            describe(name).contains("idempotency_key"),
            "{name} must tell an agent about idempotency_key"
        );
    }

    // A replay and a 409 both answer without the current state, so every
    // keyed transaction verb sends the agent to the read that has it.
    for name in [
        "transactions_capture",
        "transactions_reversal",
        "transactions_tip_adjust",
        "transactions_ach_hold",
        "transactions_ach_release",
        "transactions_credit",
    ] {
        let desc = describe(name);
        assert!(
            desc.contains("transactions_get") && desc.contains("not the current state"),
            "{name} must say a replay is not the current state and name transactions_get: {desc}"
        );
    }
    for name in [
        "transactions_capture",
        "transactions_reversal",
        "transactions_tip_adjust",
        "transactions_ach_hold",
        "transactions_ach_release",
    ] {
        assert!(
            describe(name).contains("is not proof that anything happened"),
            "{name} must say a 409 is not proof the action happened"
        );
    }

    let _ = child.kill();
    let _ = child.wait();
}

/// `instructions` reaches the model once, with the server's capabilities and
/// before any tool description, so its retry rule frames every later call and
/// has to send an agent where the creates' own descriptions send it. The rule
/// is stated as re-sending the reference, and `vary` appears nowhere in the
/// string, so its absence is the check.
#[test]
fn the_handshake_instructions_and_the_creates_agree_on_the_reference_id() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute2(&dir, r#"{"object":"ping"}"#, 0);
    let mut child = start(&fake, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let instructions = handshake(&mut stdin, &mut reader)["instructions"]
        .as_str()
        .expect("the handshake carries instructions")
        .to_string();

    assert!(
        !instructions.to_lowercase().contains("vary"),
        "the instructions must not tell an agent to vary a reference: {instructions}"
    );
    assert!(
        instructions.contains("Re-send the SAME `reference_id`"),
        "the instructions must tell an agent to re-send the same reference: {instructions}"
    );

    stdin
        .write_all(jsonrpc(2, "tools/list", json!({})).as_bytes())
        .unwrap();
    let listed = read_one_frame(&mut reader);
    let tools = listed["result"]["tools"].as_array().unwrap();
    for name in ["transactions_create", "pos_create", "pos_reversal"] {
        let desc = tools
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing from tools/list"))
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or_else(|| panic!("{name} has no description"));
        assert!(
            desc.contains("SAME `reference_id`"),
            "{name} and the instructions must agree on re-sending the same reference: {desc}"
        );
    }

    let _ = child.kill();
    let _ = child.wait();
}

/// `instructions` states that a decline is a success and names the field
/// that reports it; the money-moving creates' own descriptions carry the
/// same claim, so the two are checked against each other.
#[test]
fn the_handshake_instructions_and_the_creates_agree_on_the_decline_rule() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute2(&dir, r#"{"object":"ping"}"#, 0);
    let mut child = start(&fake, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let instructions = handshake(&mut stdin, &mut reader)["instructions"]
        .as_str()
        .expect("the handshake carries instructions")
        .to_string();

    assert!(
        instructions.contains("A declined payment is a SUCCESS, not an error"),
        "the instructions must state that a decline is a success, not an error: {instructions}"
    );
    assert!(
        instructions.contains("read `data.transactionStatus` to learn whether money moved"),
        "the instructions must direct the reader to read data.transactionStatus, not merely \
         name the field: {instructions}"
    );
    // "never" is the direction that matters here; a swap to "always" would
    // keep every other word in the sentence, so the check pins the exact
    // clause rather than the surrounding vocabulary.
    assert!(
        instructions.contains("an exit code never tells you"),
        "the instructions must say the exit code never reports the outcome: {instructions}"
    );

    stdin
        .write_all(jsonrpc(2, "tools/list", json!({})).as_bytes())
        .unwrap();
    let listed = read_one_frame(&mut reader);
    let tools = listed["result"]["tools"].as_array().unwrap();
    for name in ["transactions_create", "transactions_credit"] {
        let desc = tools
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing from tools/list"))
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or_else(|| panic!("{name} has no description"));
        assert!(
            desc.contains("DECLINE IS A SUCCESS") && desc.contains("data.transactionStatus"),
            "{name} and the instructions must agree that a decline is a success reported in \
             data.transactionStatus: {desc}"
        );
    }

    let _ = child.kill();
    let _ = child.wait();
}

/// `instructions` states the failure envelope's shape, that `kind` is read
/// before `status`, and that a schema refusal is answered before the CLI
/// runs with a plain message carrying no `kind`. A call missing a tool's own
/// required field is checked against that last claim directly: whether the
/// CLI was spawned and whether the refusal parses as the envelope at all.
#[test]
fn the_handshake_instructions_agree_with_an_actual_schema_refusal() {
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("spawned");
    let path = dir.path().join("flute2");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\ncat >/dev/null\ntouch '{}'\nprintf '{{\"object\":\"transaction\"}}'\n",
            marker.display()
        ),
    )
    .unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();

    let mut child = start(&path, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let instructions = handshake(&mut stdin, &mut reader)["instructions"]
        .as_str()
        .expect("the handshake carries instructions")
        .to_string();

    assert!(
        instructions.contains("`{kind, message, status?, correlation_id?}`"),
        "the instructions must state the failure envelope's shape: {instructions}"
    );
    assert!(
        instructions.contains("branch on `kind` first and then on `status`"),
        "the instructions must say kind is read before status: {instructions}"
    );
    assert!(
        instructions
            .contains("is refused before the CLI runs and answers a plain message with no `kind`"),
        "the instructions must say a schema refusal carries no kind and never reaches the CLI: \
         {instructions}"
    );

    // transactions_create requires `amount`; omitting it must be refused the
    // way the instructions describe, without spawning the CLI.
    let refused = call_tool(
        &mut stdin,
        &mut reader,
        2,
        "transactions_create",
        json!({ "payment_processor_id": "p1" }),
    );
    assert_eq!(refused["result"]["isError"], true);
    assert!(
        !marker.exists(),
        "a schema refusal must be answered before the CLI runs, matching the instructions"
    );
    let text = refused["result"]["content"][0]["text"]
        .as_str()
        .expect("the refusal carries text content");
    assert!(
        serde_json::from_str::<Value>(text).is_err(),
        "a schema refusal must be a plain message, not a JSON envelope carrying `kind`: {text}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// The closing paragraph names the production write guard's variable and the
/// credential variables. Both are checked against what a guarded production
/// instance and an auth failure actually return, and the profile/keychain
/// sentences — which nothing else in the suite exercises — are pinned as
/// literals.
/// The address that passes sandbox address verification is named on a sandbox
/// instance and nowhere else.
#[test]
fn only_a_sandbox_instance_names_the_address_that_passes_verification() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute2(&dir, r#"{"object":"ping"}"#, 0);
    for (profile, named) in [("sandbox", true), ("production", false)] {
        let mut child = start(&fake, profile);
        let mut stdin = child.stdin.take().unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let instructions = handshake(&mut stdin, &mut reader)["instructions"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            instructions.contains("123 Test St"),
            named,
            "{profile}: {instructions}"
        );
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[test]
fn the_handshake_instructions_agree_with_the_guard_and_credential_variables() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute2(&dir, r#"{"object":"ping"}"#, 0);
    let mut prod = start(&fake, "production");
    let mut prod_stdin = prod.stdin.take().unwrap();
    let mut prod_reader = BufReader::new(prod.stdout.take().unwrap());
    let instructions = handshake(&mut prod_stdin, &mut prod_reader)["instructions"]
        .as_str()
        .expect("the handshake carries instructions")
        .to_string();

    assert!(
        instructions
            .contains("The profile is pinned at server start; run one instance per environment."),
        "the instructions must state the profile is pinned per instance: {instructions}"
    );
    assert!(
        instructions.contains("FLUTE2_CLIENT_ID/FLUTE2_CLIENT_SECRET"),
        "the instructions must name the credential variables: {instructions}"
    );
    assert!(
        instructions.contains("or its own keychain"),
        "the instructions must name the keychain fallback: {instructions}"
    );
    assert!(
        instructions.contains("every write is refused unless FLUTE2_MCP_ALLOW_PROD_WRITES=1"),
        "the instructions must state the production write guard: {instructions}"
    );

    let write = call_tool(
        &mut prod_stdin,
        &mut prod_reader,
        2,
        "customers_delete",
        json!({ "id": "c1" }),
    );
    assert_eq!(write["result"]["isError"], true);
    let guard_message = content(&write)["message"].as_str().unwrap().to_string();
    assert!(
        guard_message.contains("FLUTE2_MCP_ALLOW_PROD_WRITES"),
        "a guarded production write must name the same variable the instructions do: \
         {guard_message}"
    );
    let _ = prod.kill();
    let _ = prod.wait();

    let auth_fake = write_fake_flute2(
        &dir,
        r#"{"kind":"auth","message":"no credentials configured"}"#,
        2,
    );
    let mut sandbox = start(&auth_fake, "sandbox");
    let mut sandbox_stdin = sandbox.stdin.take().unwrap();
    let mut sandbox_reader = BufReader::new(sandbox.stdout.take().unwrap());
    handshake(&mut sandbox_stdin, &mut sandbox_reader);
    let auth_failure = call_tool(
        &mut sandbox_stdin,
        &mut sandbox_reader,
        2,
        "transactions_list",
        json!({}),
    );
    assert_eq!(auth_failure["result"]["isError"], true);
    let auth_message = content(&auth_failure)["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        auth_message.contains("FLUTE2_CLIENT_ID") && auth_message.contains("FLUTE2_CLIENT_SECRET"),
        "an auth failure must name the same credential variables the instructions do: \
         {auth_message}"
    );

    let _ = sandbox.kill();
    let _ = sandbox.wait();
}

#[test]
fn an_auth_failure_surfaces_the_kind_and_the_credential_variables() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute2(
        &dir,
        r#"{"kind":"auth","message":"no credentials configured"}"#,
        2,
    );
    let mut child = start(&fake, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    handshake(&mut stdin, &mut reader);

    let called = call_tool(&mut stdin, &mut reader, 2, "transactions_list", json!({}));
    assert_eq!(called["result"]["isError"], true);
    let payload = content(&called);
    assert_eq!(payload["kind"], "auth");
    let msg = payload["message"].as_str().unwrap();
    assert!(msg.contains("FLUTE2_CLIENT_ID") && msg.contains("FLUTE2_CLIENT_SECRET"));

    let _ = child.kill();
    let _ = child.wait();
}

/// A production instance refuses a write and still serves reads. The fake CLI
/// exits 0, so a guarded write that reached it would surface a success instead
/// of the guard's `client` message.
#[test]
fn production_refuses_a_write_and_still_serves_a_read() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute2(&dir, r#"{"object":"transaction_list","data":[]}"#, 0);
    let mut child = start(&fake, "production");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    handshake(&mut stdin, &mut reader);

    let write = call_tool(
        &mut stdin,
        &mut reader,
        2,
        "customers_delete",
        json!({ "id": "c1" }),
    );
    assert_eq!(write["result"]["isError"], true);
    let payload = content(&write);
    assert_eq!(payload["kind"], "client");
    assert!(
        payload["message"]
            .as_str()
            .unwrap()
            .contains("FLUTE2_MCP_ALLOW_PROD_WRITES")
    );

    let read = call_tool(&mut stdin, &mut reader, 3, "transactions_list", json!({}));
    assert_eq!(read["result"]["isError"], false);

    let _ = child.kill();
    let _ = child.wait();
}

/// The suite's result is a property of the tree, not of the shell that runs
/// it: re-running the guard test with `FLUTE2_MCP_ALLOW_PROD_WRITES` set in
/// this process's own environment must not change its outcome, because the
/// spawn helper does not forward this process's environment to the server.
#[test]
fn the_guard_test_ignores_an_inherited_allow_writes_variable() {
    let exe = std::env::current_exe().expect("this test binary's own path");
    let out = Command::new(&exe)
        .env("FLUTE2_MCP_ALLOW_PROD_WRITES", "1")
        .args([
            "--exact",
            "production_refuses_a_write_and_still_serves_a_read",
        ])
        .output()
        .expect("re-run the guard test under an inherited write-guard variable");
    assert!(
        out.status.success(),
        "the guard test must pass whatever this process's own environment sets:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("1 passed"),
        "the exact filter must have matched and run the guard test:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// `page_size` arrives quoted from many clients, and the child must receive it
/// as a bare integer. The fake CLI echoes its own argv back as the envelope.
#[test]
fn a_quoted_page_size_reaches_the_child_as_an_integer() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("flute2");
    fs::write(
        &path,
        "#!/bin/sh\ncat >/dev/null\nprintf '{\"object\":\"argv\",\"data\":\"%s\"}' \"$*\"\nexit 0\n",
    )
    .unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();

    let mut child = start(&path, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    handshake(&mut stdin, &mut reader);

    let called = call_tool(
        &mut stdin,
        &mut reader,
        2,
        "customers_list",
        json!({ "page_size": "25" }),
    );
    let argv = content(&called)["data"].as_str().unwrap().to_string();
    assert!(
        argv.contains("--page-size 25"),
        "the child received: {argv}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// A refused argument names its field, even when another argument beside it
/// arrives as a quoted integer the tool accepts.
#[test]
fn a_mistyped_argument_is_named_in_the_refusal() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute2(&dir, r#"{"object":"x"}"#, 0);
    let mut child = start(&fake, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    handshake(&mut stdin, &mut reader);

    for (id, name, arguments, field) in [
        (
            2,
            "transactions_capture",
            json!({ "transaction_id": "t1", "amount": 1.5 }),
            "`amount`",
        ),
        (
            3,
            "customers_list",
            json!({ "page_size": -1 }),
            "`page_size`",
        ),
        (
            4,
            "customers_list",
            json!({ "all": "yes", "page_size": "25" }),
            "`all`",
        ),
    ] {
        let frame = call_tool(&mut stdin, &mut reader, id, name, arguments);
        let text = frame["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(frame["result"]["isError"], true);
        assert!(
            text.contains(field),
            "{name} refusal must name {field}: {text}"
        );
    }

    let _ = child.kill();
    let _ = child.wait();
}

/// The server's `RUST_LOG` is stripped from the child, so the server's log
/// level does not turn the CLI's stderr into noise. Everything else is
/// inherited, which is how credentials reach the CLI.
#[test]
fn the_child_does_not_inherit_rust_log() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("flute2");
    fs::write(
        &path,
        "#!/bin/sh\ncat >/dev/null\nprintf '{\"object\":\"env\",\"data\":{\"log\":\"%s\",\"bin\":\"%s\"}}' \"$RUST_LOG\" \"$FLUTE2_BIN\"\nexit 0\n",
    )
    .unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();

    let mut child = start(&path, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    handshake(&mut stdin, &mut reader);

    let called = call_tool(&mut stdin, &mut reader, 2, "ping", json!({}));
    let data = content(&called)["data"].clone();
    assert_eq!(data["log"], "", "RUST_LOG must not reach the child");
    assert_ne!(
        data["bin"], "",
        "every other variable must still be inherited"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// An `initialize` the server cannot deserialize is answered with a JSON-RPC
/// error carrying the request's own id, and the server stays up: a client that
/// waits for the answer and corrects its handshake is served.
#[test]
fn a_bad_initialize_is_answered_and_a_corrected_one_is_served() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute2(&dir, r#"{"object":"ping"}"#, 0);
    let mut child = start(&fake, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    stdin
        .write_all(
            jsonrpc(
                1,
                "initialize",
                json!({
                    "protocolVersion": 42,
                    "capabilities": {},
                    "clientInfo": { "name": "e2e", "version": "0" }
                }),
            )
            .as_bytes(),
        )
        .unwrap();
    let refused = read_one_frame(&mut reader);
    assert_eq!(refused["id"], 1, "the error must carry the request's id");
    assert_eq!(refused["error"]["code"], -32602);
    let message = refused["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("protocolVersion"),
        "the error must name the fault: {message}"
    );

    // The same stdin, a well-formed handshake, answered by the same process.
    handshake_with_id(&mut stdin, &mut reader, 2);

    let called = call_tool(&mut stdin, &mut reader, 3, "ping", json!({}));
    assert_eq!(called["result"]["isError"], false);

    let _ = child.kill();
    let _ = child.wait();
}

/// A request that arrives before the handshake is answered rather than
/// dropped, and the answer names the method it received.
#[test]
fn a_request_before_initialize_is_answered_with_an_error_frame() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute2(&dir, r#"{"object":"ping"}"#, 0);
    let mut child = start(&fake, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    stdin
        .write_all(jsonrpc(9, "tools/list", json!({})).as_bytes())
        .unwrap();
    let refused = read_one_frame(&mut reader);
    assert_eq!(refused["id"], 9);
    assert_eq!(refused["error"]["code"], -32600);
    let message = refused["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("tools/list") && message.contains("initialize"),
        "the error must name both the method it wanted and the one it got: {message}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// A signal during a call takes the `flute2` child with the server, so a
/// charge in flight is not left running with nothing to read its result.
#[test]
fn a_signal_during_a_call_takes_the_child_with_it() {
    for signal in ["-TERM", "-INT"] {
        a_signal_kills_the_child(signal);
    }
}

fn a_signal_kills_the_child(signal: &str) {
    let dir = TempDir::new().unwrap();
    let pidfile = dir.path().join("child.pid");
    let path = dir.path().join("flute2");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s' \"$$\" > {}\nsleep 20\nprintf '{{\"object\":\"ping\"}}'\n",
            pidfile.display()
        ),
    )
    .unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();

    let mut child = start(&path, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    handshake(&mut stdin, &mut reader);

    stdin
        .write_all(jsonrpc(2, "tools/call", json!({ "name": "ping", "arguments": {} })).as_bytes())
        .unwrap();
    assert!(
        wait_until(|| pidfile.exists() && !fs::read_to_string(&pidfile).unwrap().is_empty()),
        "the fake CLI never started"
    );
    let cli_pid: u32 = fs::read_to_string(&pidfile)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(alive(cli_pid), "the fake CLI must still be running");

    Command::new("kill")
        .args([signal, &child.id().to_string()])
        .status()
        .expect("send the signal");

    assert!(
        wait_until(|| !alive(cli_pid)),
        "the flute2 child outlived the server on {signal}"
    );
    assert!(
        wait_until(|| child.try_wait().unwrap().is_some()),
        "the server did not exit on {signal}"
    );
    let status = child.wait().unwrap();
    let by_signal = if signal == "-TERM" { 15 } else { 2 };
    assert!(
        status.code() == Some(0) || status.signal() == Some(by_signal),
        "{signal} left the server at {status:?}"
    );
}

/// No `RUST_LOG` may put tool arguments in the log: a card number, an expiry
/// and a CVV all arrive as arguments. A span-qualified directive names a
/// narrower scope than a bare target and so outranks one, which is why the
/// levelled shapes below are here beside the plain ones. The second field is
/// whether the filter admits this crate's own `debug`, which carries the child
/// stderr that `FLUTE2_MCP_DEBUG` asks for: a span-qualified directive admits
/// every event inside that span, this crate's included.
#[test]
fn no_rust_log_setting_carries_tool_arguments() {
    for (filter, keeps_our_debug) in [
        ("info", false),
        ("debug", true),
        ("trace", true),
        ("debug,rmcp::service=trace", true),
        ("debug,rmcp::service[{id}]=trace", true),
        ("rmcp::service[serve_inner]=debug", true),
        ("rmcp[serve_inner]=trace", true),
    ] {
        the_log_omits_tool_arguments(filter, keeps_our_debug);
    }
}

fn the_log_omits_tool_arguments(filter: &str, keeps_our_debug: bool) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("flute2");
    fs::write(
        &path,
        "#!/bin/sh\ncat >/dev/null\necho 'child-stderr-marker' >&2\nprintf '{\"object\":\"payment_method\"}'\n",
    )
    .unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();

    let mut child = start_with_env(
        &path,
        "sandbox",
        &[("RUST_LOG", filter), ("FLUTE2_MCP_DEBUG", "1")],
    );
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    handshake(&mut stdin, &mut reader);

    let called = call_tool(
        &mut stdin,
        &mut reader,
        2,
        "payment_methods_add_card",
        json!({ "card": "4111111111111111", "exp": "12/30", "cvv": "987" }),
    );
    assert_eq!(called["result"]["isError"], false);

    // Closing stdin drains the call and exits 0.
    drop(stdin);
    assert!(child.wait().unwrap().success());
    let mut log = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut log)
        .unwrap();

    for secret in ["4111111111111111", "12/30", "cvv"] {
        assert!(
            !log.contains(secret),
            "RUST_LOG={filter} carries a tool argument ({secret}): {log}"
        );
    }
    assert!(
        log.contains("payment_methods_add_card"),
        "RUST_LOG={filter} loses the name of the tool that was called: {log}"
    );
    assert_eq!(
        log.contains("child-stderr-marker"),
        keeps_our_debug,
        "RUST_LOG={filter} decides this crate's own debug log: {log}"
    );
}

/// The clamp only takes away the levels below `info`: what `rmcp` says at
/// `info` and above stays under the operator's `RUST_LOG`.
#[test]
fn a_lowered_rust_log_silences_rmcp() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute2(&dir, r#"{"object":"ping"}"#, 0);
    let mut child = start_with_env(&fake, "sandbox", &[("RUST_LOG", "warn")]);
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    handshake(&mut stdin, &mut reader);

    drop(stdin);
    assert!(child.wait().unwrap().success());
    let mut log = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut log)
        .unwrap();
    assert_eq!(
        log, "",
        "RUST_LOG=warn must silence the handshake log: {log}"
    );
}

/// A notification is never answered: a JSON-RPC error carries an id, and a
/// notification has none.
#[test]
fn a_notification_before_initialize_is_not_answered() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute2(&dir, r#"{"object":"ping"}"#, 0);
    let mut child = start(&fake, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    stdin
        .write_all(jsonrpc_notify("notifications/initialized", json!({})).as_bytes())
        .unwrap();
    drop(stdin);
    assert!(
        wait_until(|| child.try_wait().unwrap().is_some()),
        "the server did not exit when stdin closed"
    );

    let mut answered = String::new();
    stdout.read_to_string(&mut answered).unwrap();
    assert_eq!(answered, "", "a notification was answered: {answered}");
}

/// `--profile` and `--output` are clap globals whose later occurrence wins, so
/// a caller value the CLI reads as an option redirects the child. Such a value
/// is refused before any spawn, and an ordinary id on the same field still
/// reaches the CLI.
#[test]
fn an_option_shaped_argument_is_refused_before_the_cli_is_spawned() {
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("spawned");
    let path = dir.path().join("flute2");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\ncat >/dev/null\ntouch '{}'\nprintf '{{\"object\":\"customer\"}}'\n",
            marker.display()
        ),
    )
    .unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();

    let mut child = start(&path, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    handshake(&mut stdin, &mut reader);

    let refused = call_tool(
        &mut stdin,
        &mut reader,
        2,
        "customers_get",
        json!({ "id": "--profile=production" }),
    );
    assert_eq!(refused["result"]["isError"], true);
    let payload = content(&refused);
    assert_eq!(payload["kind"], "client");
    let message = payload["message"].as_str().unwrap();
    assert!(message.contains("id"), "{message}");
    assert!(message.contains("--profile=production"), "{message}");
    assert!(!marker.exists(), "the refused call spawned the CLI");

    let allowed = call_tool(
        &mut stdin,
        &mut reader,
        3,
        "customers_get",
        json!({ "id": "cus_01HZ" }),
    );
    assert_eq!(allowed["result"]["isError"], false);
    assert!(marker.exists(), "an ordinary id must still reach the CLI");

    let _ = child.kill();
    let _ = child.wait();
}

/// Drives the real CLI named by `FLUTE2_SMOKE_BIN` with no credentials and no
/// keychain, so every call stops before the network. A call whose argv the CLI
/// parses answers `auth`; a flag the CLI does not accept answers `client`.
#[test]
#[ignore = "needs a built flute2 CLI; run with FLUTE2_SMOKE_BIN=/path/to/flute2 cargo test -- --ignored"]
fn the_real_cli_accepts_every_flag_these_calls_emit() {
    let bin = std::env::var("FLUTE2_SMOKE_BIN").expect("FLUTE2_SMOKE_BIN names the flute2 CLI");
    let home = TempDir::new().unwrap();
    let home_path = home.path().to_str().unwrap();
    let mut child = start_with_env(
        std::path::Path::new(&bin),
        "sandbox",
        &[
            ("HOME", home_path),
            ("FLUTE2_NO_KEYCHAIN", "1"),
            ("FLUTE2_NO_UPDATE_CHECK", "1"),
        ],
    );
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    handshake(&mut stdin, &mut reader);

    let calls = [
        (
            "settings_update_autofill",
            json!({
                "l2_tax_rate": "7.25", "l3_shipping_rate": "1", "l3_duty_rate": "1",
                "product_name": "Widget", "product_code": "SKU-1", "product_unit": "EA",
                "product_unit_price": "19.99", "product_quantity": "1", "product_discount": "0"
            }),
        ),
        ("transactions_list", json!({ "asc": true })),
        ("transactions_list", json!({ "desc": true })),
        ("terminals_list", json!({ "asc": true })),
        ("terminals_list", json!({ "desc": true })),
        ("pos_list", json!({ "asc": true })),
        ("pos_list", json!({ "desc": true })),
        ("payment_links_list", json!({ "asc": true })),
        ("payment_links_list", json!({ "desc": true })),
        ("customers_list", json!({ "desc": true })),
        (
            "payment_methods_list",
            json!({ "customer_id": "cus_1", "desc": true }),
        ),
        ("settlements_list", json!({ "asc": true })),
    ];
    for (id, (name, arguments)) in (2..).zip(calls) {
        let frame = call_tool(&mut stdin, &mut reader, id, name, arguments.clone());
        let payload = content(&frame);
        assert_eq!(payload["kind"], "auth", "{name} {arguments}: {payload}");
    }

    let _ = child.kill();
    let _ = child.wait();
}

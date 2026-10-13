//! The readme is the only place an operator learns the tool surface, so it is
//! checked against the server rather than against a second hand-kept list.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn tool_names() -> Vec<String> {
    let bin = assert_cmd::cargo::cargo_bin("flute2-mcp");
    let mut child = Command::new(bin)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("FLUTE2_BIN", "/bin/sh")
        .env("FLUTE2_PROFILE", "sandbox")
        .env("RUST_LOG", "warn")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn server");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    let frame = |id: u64, method: &str, params: Value| {
        format!(
            "{}\n",
            serde_json::to_string(
                &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
            )
            .unwrap()
        )
    };
    let read = |reader: &mut BufReader<_>| -> Value {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read frame");
        serde_json::from_str(line.trim()).expect("parse frame")
    };

    stdin
        .write_all(
            frame(
                1,
                "initialize",
                json!({
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": { "name": "docs", "version": "0" }
                }),
            )
            .as_bytes(),
        )
        .unwrap();
    let _ = read(&mut reader);
    stdin
        .write_all(
            format!(
                "{}\n",
                serde_json::to_string(
                    &json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}})
                )
                .unwrap()
            )
            .as_bytes(),
        )
        .unwrap();
    stdin
        .write_all(frame(2, "tools/list", json!({})).as_bytes())
        .unwrap();
    let listed = read(&mut reader);
    let names = listed["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    let _ = child.kill();
    let _ = child.wait();
    names
}

/// The write set is the tools whose function calls `guard_write`, read off the
/// tool functions themselves.
fn write_tools() -> BTreeSet<String> {
    let mut writes = BTreeSet::new();
    for entry in std::fs::read_dir("src/tools").expect("src/tools") {
        let source =
            std::fs::read_to_string(entry.expect("dir entry").path()).expect("tool source");
        for function in source.split("pub async fn ").skip(1) {
            let (name, body) = function.split_once('(').expect("function signature");
            if body.contains("guard_write(") {
                writes.insert(name.to_string());
            }
        }
    }
    writes
}

/// The Tools section is the operator's read/write contract: which verbs run on
/// a guarded production instance and which are refused. The count it states,
/// the names it lists and the (R)/(W) on each are checked against the server.
#[test]
fn the_readme_publishes_the_census_the_server_serves() {
    let readme = std::fs::read_to_string("readme.md").expect("readme.md");
    let section = readme
        .split("\n## Tools\n")
        .nth(1)
        .expect("readme.md has no Tools section")
        .split("\n## ")
        .next()
        .unwrap();
    let stated: usize = section
        .split_whitespace()
        .next()
        .expect("the Tools section is empty")
        .parse()
        .expect("the Tools section does not open with a tool count");

    // A name is a census entry when an (R) or a (W) follows its backticks; the
    // section names tools in prose as well.
    let mut documented = BTreeMap::new();
    let mut pieces = section.split('`');
    pieces.next();
    while let (Some(name), Some(after)) = (pieces.next(), pieces.next()) {
        let after = after.trim_start();
        if after.starts_with("(R)") || after.starts_with("(W)") {
            documented.insert(name.to_string(), &after[..3]);
        }
    }

    let names: BTreeSet<String> = tool_names().into_iter().collect();
    let writes = write_tools();
    assert!(
        writes.is_subset(&names),
        "src/tools guards a write the server does not serve: {:?}",
        writes.difference(&names).collect::<Vec<_>>()
    );
    let served: BTreeMap<String, &str> = names
        .iter()
        .map(|name| {
            let label = if writes.contains(name) { "(W)" } else { "(R)" };
            (name.clone(), label)
        })
        .collect();

    assert_eq!(
        stated,
        served.len(),
        "the Tools section states {stated} tools and the server serves {}",
        served.len()
    );
    let mut diverged = Vec::new();
    for (name, label) in &served {
        match documented.get(name) {
            None => diverged.push(format!(
                "{name} {label} is served and the Tools section does not list it"
            )),
            Some(listed) if listed != label => diverged.push(format!(
                "{name} is listed {listed} and the server serves it as {label}"
            )),
            _ => {}
        }
    }
    for name in documented.keys().filter(|name| !served.contains_key(*name)) {
        diverged.push(format!(
            "{name} is listed in the Tools section and the server does not serve it"
        ));
    }
    assert!(
        diverged.is_empty(),
        "readme.md's tool census and the server disagree:\n{}",
        diverged.join("\n")
    );
}

#[test]
fn the_readme_documents_every_environment_variable() {
    let readme = std::fs::read_to_string("readme.md").expect("readme.md");
    for var in [
        "FLUTE2_PROFILE",
        "FLUTE2_BIN",
        "FLUTE2_MERCHANT_ID",
        "FLUTE2_MCP_TIMEOUT_SECS",
        "FLUTE2_MCP_DEBUG",
        "FLUTE2_MCP_ALLOW_PROD_WRITES",
        "FLUTE2_CLIENT_ID",
        "FLUTE2_CLIENT_SECRET",
        "RUST_LOG",
    ] {
        assert!(readme.contains(var), "readme.md does not document {var}");
    }
}

/// Every client-configuration block sets the two credential variables the
/// paragraph above them says avoids the per-call keychain prompt.
#[test]
fn every_client_config_block_sets_the_credential_variables() {
    let readme = std::fs::read_to_string("readme.md").expect("readme.md");
    let section = readme
        .split("## Client configuration")
        .nth(1)
        .expect("readme.md has no Client configuration section")
        .split("## Tools")
        .next()
        .unwrap();
    // Fenced code blocks are the odd-indexed segments of a split on the fence
    // marker: prose, block, prose, block, ...
    let blocks: Vec<&str> = section
        .split("```")
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, b)| b)
        .collect();
    assert_eq!(blocks.len(), 4, "expected four client-configuration blocks");
    for block in blocks {
        assert!(
            block.contains("FLUTE2_CLIENT_ID") && block.contains("FLUTE2_CLIENT_SECRET"),
            "a client-configuration block does not set the credential variables:\n{block}"
        );
    }
}

/// The retry paragraph is the operator's copy of the recovery each create's own
/// description prescribes, so it names the read each one is reconciled with,
/// and names the create whose reconcile is refused as the exception.
#[test]
fn the_retry_paragraph_names_the_reconcile_for_each_create() {
    let readme = std::fs::read_to_string("readme.md").expect("readme.md");
    let paragraph = readme
        .split("Money-moving creates are not safe to retry")
        .nth(1)
        .expect("readme.md has no retry paragraph")
        .split("\n\n")
        .next()
        .unwrap();
    for (create, read_back) in [
        ("transactions_create", "transactions_list"),
        ("pos_create", "pos_get"),
        ("pos_reversal", "pos_get"),
        ("transactions_credit", "transactions_list"),
        ("payment_links_create", "payment_links_list"),
        ("api_keys_create", "api_keys_list"),
    ] {
        assert!(
            paragraph.contains(create),
            "the retry paragraph does not name {create}"
        );
        assert!(
            paragraph.contains(read_back),
            "the retry paragraph does not name {read_back}, the read {create} is reconciled with"
        );
    }
}

/// The rule an agent most often gets wrong belongs in the operator's document
/// as well as in the tool descriptions.
#[test]
fn the_readme_states_that_a_decline_is_a_success() {
    let readme = std::fs::read_to_string("readme.md").expect("readme.md");
    assert!(readme.contains("transactionStatus"));
    assert!(readme.to_lowercase().contains("decline"));
}

/// The Homebrew installer only reaches a user if cargo-dist has a tap to push
/// the formula to. Listing `homebrew` in `installers` alone attaches the `.rb`
/// to the release page, and `brew install getflute/flute-cli-v2-mcp/flute2-mcp` then
/// fails at tap time.
#[test]
fn the_dist_config_names_the_tap_the_readme_installs_from() {
    let dist = std::fs::read_to_string("dist-workspace.toml").expect("dist-workspace.toml");
    let readme = std::fs::read_to_string("readme.md").expect("readme.md");
    assert!(
        dist.contains(r#"tap = "getflute/homebrew-flute-cli-v2-mcp""#),
        "dist-workspace.toml names no tap"
    );
    assert!(
        dist.contains(r#"publish-jobs = ["homebrew"]"#),
        "dist-workspace.toml has no homebrew publish job"
    );
    assert!(
        readme.contains("brew install getflute/flute-cli-v2-mcp/flute2-mcp"),
        "readme.md does not install from the tap"
    );
}

/// The installers the config generates are the ones the readme tells a user to
/// run. The installers list is matched as a whole line: `homebrew` also appears
/// in the `tap` and `publish-jobs` keys, and `shell` is a substring of
/// `powershell`, so a bare substring search pins neither.
#[test]
fn the_dist_config_and_readme_agree_on_the_installers() {
    let dist = std::fs::read_to_string("dist-workspace.toml").expect("dist-workspace.toml");
    let readme = std::fs::read_to_string("readme.md").expect("readme.md");
    assert!(
        dist.contains(r#"installers = ["shell", "powershell", "homebrew"]"#),
        "dist-workspace.toml does not generate the shell, powershell and homebrew installers"
    );
    assert!(readme.contains("flute2-mcp-installer.sh"));
    assert!(readme.contains("flute2-mcp-installer.ps1"));
}

/// A branch or tag is a ref its owner can move; only a commit hash is
/// immutable. `release.yml` pins every action it uses this way, and so must
/// `ci.yml`, whose jobs also check the source out and run against it.
#[test]
fn ci_workflow_actions_are_pinned_to_commit_hashes() {
    let ci = std::fs::read_to_string(".github/workflows/ci.yml").expect("ci.yml");
    let mut checked = 0;
    for line in ci.lines().filter(|l| l.contains("uses:")) {
        let reference = line
            .split("uses:")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        let pin = reference.rsplit('@').next().unwrap();
        assert!(
            pin.len() == 40 && pin.chars().all(|c| c.is_ascii_hexdigit()),
            "{reference} is not pinned to a commit hash"
        );
        checked += 1;
    }
    assert_eq!(checked, 3, "expected three `uses:` steps in ci.yml");
}

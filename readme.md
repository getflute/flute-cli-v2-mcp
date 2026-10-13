# flute2-mcp

An MCP server that drives the [`flute2`](https://github.com/getflute/flute-cli-v2)
CLI, so an agent can operate the Flute v2 payments API over stdio without
speaking HTTP. `flute2` owns the API, OAuth, decimals and redaction; this server
forwards its JSON envelopes.

It ships beside [`flute-cli-mcp`](https://github.com/getflute/flute-cli-mcp),
which drives the v1 `flute` CLI. Neither knows the other exists, and both can be
installed.

## How it works

One `flute2` child process per tool call, always
`flute2 --profile <pinned> --output json …`. On success the CLI's envelope is
returned unchanged, except that a child exiting zero with no stdout at all
returns a bare JSON `null` body, and `auth_status` answers a flat object this
server builds rather than the CLI's envelope. On failure the CLI's `{kind,
message, status?, correlation_id?}` envelope is returned with `isError` set.

## Install

**Homebrew (Apple Silicon macOS / x86_64 Linux)**

```sh
brew install getflute/flute-cli-v2-mcp/flute2-mcp
```

**Shell script (Apple Silicon macOS / x86_64 Linux — installs from GitHub Releases)**

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/getflute/flute-cli-v2-mcp/releases/latest/download/flute2-mcp-installer.sh | sh
```

**PowerShell (x86_64 Windows — installs from GitHub Releases)**

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/getflute/flute-cli-v2-mcp/releases/latest/download/flute2-mcp-installer.ps1 | iex"
```

**From source**

```sh
cargo install --path .
```

Prereq: install `flute2` 1.1.0 or later (`brew install getflute/flute-cli-v2/flute2`,
or see [getflute/flute-cli-v2](https://github.com/getflute/flute-cli-v2)) and put
it on `PATH`, or name it with `FLUTE2_BIN`.

## Run

```sh
FLUTE2_PROFILE=sandbox flute2-mcp
```

The server speaks MCP on stdin and stdout; logs go to stderr. A configuration
error exits `2` with one stderr line before any request is served.

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `FLUTE2_PROFILE` | `sandbox` | `sandbox`, or `production`/`prod`. Pinned at startup and passed to every child as `--profile`. |
| `FLUTE2_BIN` | `which flute2` | Absolute path to the `flute2` binary. Startup exits 2 if neither resolves to an executable file. |
| `FLUTE2_MERCHANT_ID` | unset | Merchant id for `api_keys_create` (required there) and `api_keys_list` (optional filter). A per-call `merchant_id` overrides it. |
| `FLUTE2_MCP_TIMEOUT_SECS` | `30` | Per-call child timeout, a positive integer. |
| `FLUTE2_MCP_DEBUG` | off | `1`/`true`/`yes`/`on` routes the child's stderr into this server's tracing at debug level, so it is recorded only when `RUST_LOG` admits this crate at `debug`. |
| `FLUTE2_MCP_ALLOW_PROD_WRITES` | off | `1`/`true`/`yes`/`on` lifts the production write guard. Any other value, `false` and `0` included, keeps it on. |
| `RUST_LOG` | `info` | tracing filter. Logs go to stderr only, and this variable is not passed to the child. |

`FLUTE2_CLIENT_ID` and `FLUTE2_CLIENT_SECRET` are never read by this server.
They are inherited by the `flute2` child, which is how an agent authenticates
without anything being stored. Alternatively run `flute2 auth login` on the
machine and let the CLI use its keychain. One child per tool call means one
keychain read per tool call, so macOS raises a credential dialog on every call
until the operator answers **Always Allow**, which grants the binary standing
access to the stored payments credential. The two variables avoid both, which is
why the client configurations below set them.

## Production write guard

Run one instance per environment. On a `production` instance every write tool
returns, without spawning the CLI:

```json
{ "kind": "client",
  "message": "refusing <tool> on production profile; set FLUTE2_MCP_ALLOW_PROD_WRITES=1 to enable writes" }
```

Reads always run. Sandbox instances never block.

## A declined payment is a success

The API answers a decline with HTTP 200, so the transaction exists, has an id,
and was refused. **Read `data.transactionStatus`.** An agent that treats a
successful tool call as "paid" will report a declined charge as a completed one.

Money-moving creates are not safe to retry without an idempotency key. Fifteen
writes take `idempotency_key`, sent as the API's `Idempotency-Key`:
`transactions_create`, `transactions_credit`, `transactions_capture`,
`transactions_reversal`, `transactions_tip_adjust`, `transactions_ach_hold`,
`transactions_ach_release`, `pos_create`, `pos_cancel`, `pos_reversal`,
`payment_links_create`, `payment_links_share`, `payment_sessions_create`,
`payment_sessions_cancel` and `settlements_close`. Pass a fresh key, e.g. a
UUID, on the first attempt; after an ambiguous failure re-send the same key with
the same arguments and the API returns the first result instead of acting twice.
The same key with different arguments answers 422, and a key is not scoped to
one tool. A replay returns the first response as it was, an error included, so
read the resource to learn its current state. A 409 means the request was
refused for the resource's current state (already captured or voided, declined,
an ACH sale in progress, or the same key still in flight) and is not proof that
anything happened; its advice to use a different idempotency key would act
again. The rest of this section is for a write sent without a key, where no
`reference_id` is an unconditional retry guard. `transactions_create` takes a
`reference_id` that is part of the API's duplicate-check key on card
authorisations: re-send the same one and the repeat is refused, vary it and a
second charge goes through. An ACH sale has no such check, and two with the same
reference both go through. Card sales sent at the same moment with one reference
all went through as well. The check is a per-merchant setting rather than a
guarantee of the endpoint, so a merchant with it switched off accepts the repeat
and charges twice — reconcile with `transactions_list` before re-issuing, then
re-send the same reference. `pos_create`'s `reference_id` is unique per merchant
forever, which makes it a claim on a name rather than a retry guard: the repeat
answers 400 `ReferenceId already exists` whether or not the first sale opened,
so reconcile with `pos_get` or `pos_list` first; `pos_reversal`'s `reference_id`
works the same way and is reconciled the same way. `transactions_credit`
requires a `reference_id` that does not dedupe, so a repeat credits a second
time — reconcile with `transactions_list` on that reference first, or, once an
ACH hold or release has overwritten it, by transaction id. Voiding an ACH sale
or reversing an ACH credit replaces its reference too, so reconcile those by
transaction id. `payment_links_create` refuses no repeat, and a second create
answers a second live link; `payment_links_list` filters on `search` rather than
on `reference_id`. `payment_sessions_create` takes a `reference_id` that neither
dedupes nor is queryable, and the class has no `list` verb, so an ambiguous
create cannot be reconciled at all. The other creates have no de-duplication
knob; on an ambiguous failure, reconcile with the matching `list` or `get`
before re-issuing — except `api_keys_create`, whose reconcile is
`api_keys_list`, which the same 403 refuses, so an ambiguous create there cannot
be reconciled either.

## Client configuration

Claude Desktop, sandbox:

```json
{
  "mcpServers": {
    "flute2": {
      "command": "/absolute/path/to/flute2-mcp",
      "env": {
        "FLUTE2_PROFILE": "sandbox",
        "FLUTE2_BIN": "/absolute/path/to/flute2",
        "FLUTE2_CLIENT_ID": "…",
        "FLUTE2_CLIENT_SECRET": "…"
      }
    }
  }
}
```

Claude Desktop, production with writes deliberately enabled:

```json
{
  "mcpServers": {
    "flute2-prod": {
      "command": "/absolute/path/to/flute2-mcp",
      "env": {
        "FLUTE2_PROFILE": "production",
        "FLUTE2_BIN": "/absolute/path/to/flute2",
        "FLUTE2_MCP_ALLOW_PROD_WRITES": "1",
        "FLUTE2_CLIENT_ID": "…",
        "FLUTE2_CLIENT_SECRET": "…"
      }
    }
  }
}
```

Codex (`~/.codex/config.toml`), sandbox:

```toml
[mcp_servers.flute2]
command = "/absolute/path/to/flute2-mcp"
env = { FLUTE2_PROFILE = "sandbox", FLUTE2_BIN = "/absolute/path/to/flute2", FLUTE2_CLIENT_ID = "…", FLUTE2_CLIENT_SECRET = "…" }
```

Codex, production with writes deliberately enabled:

```toml
[mcp_servers.flute2-prod]
command = "/absolute/path/to/flute2-mcp"
env = { FLUTE2_PROFILE = "production", FLUTE2_BIN = "/absolute/path/to/flute2", FLUTE2_MCP_ALLOW_PROD_WRITES = "1", FLUTE2_CLIENT_ID = "…", FLUTE2_CLIENT_SECRET = "…" }
```

## Tools

53 tools. **R** is a read and always runs; **W** is a write and is refused on a
guarded production instance.

**transactions** — `transactions_get` (R), `transactions_list` (R),
`transactions_calculate_amount` (R), `transactions_create` (W),
`transactions_capture` (W), `transactions_reversal` (W),
`transactions_tip_adjust` (W), `transactions_ach_hold` (W),
`transactions_ach_release` (W), `transactions_share_receipt` (W),
`transactions_credit` (W)

**customers** — `customers_get` (R), `customers_list` (R), `customers_create`
(W), `customers_update` (W), `customers_delete` (W)

**payment methods** — `payment_methods_list` (R), `payment_methods_get` (R),
`payment_methods_add_card` (W), `payment_methods_add_ach` (W),
`payment_methods_update` (W), `payment_methods_delete` (W),
`payment_methods_set_default` (W)

**payment links** — `payment_links_get` (R), `payment_links_list` (R),
`payment_links_create` (W), `payment_links_update` (W), `payment_links_delete`
(W), `payment_links_share` (W)

**payment sessions** — `payment_sessions_get` (R), `payment_sessions_create`
(W), `payment_sessions_cancel` (W)

**pos** — `pos_get` (R), `pos_list` (R), `pos_create` (W), `pos_reversal` (W),
`pos_cancel` (W), `pos_print_receipt` (W)

**terminals** — `terminals_list` (R), `terminals_status` (R)

**settlements** — `settlements_list` (R), `settlements_get` (R),
`settlements_close` (W)

**settings** — `settings_payment_config` (R), `settings_contact_info` (R),
`settings_autofill` (R), `settings_update_autofill` (W)

**api keys** — `api_keys_list` (R), `api_keys_create` (W), `api_keys_revoke` (W)

**utility** — `ping` (R), `version` (R), `auth_status` (R)

`auth login`, `auth logout`, `auth switch`, `auth token`, `update`,
`completion` and `transactions inspect` are deliberately not exposed: they are
interactive, rewrite local state, print a credential, print no envelope, or
duplicate another tool exactly. `pos create --wait` and `pos get --wait` are not
exposed either — `pos_create` returns the in-progress transaction and the agent
polls `pos_get`, so every call finishes inside the per-call timeout.

## Errors

A failure the CLI or this server reports is a JSON object with a `kind`,
returned with `isError` set. A call that violates a tool's own schema — a
missing required field, an unknown field — is refused before the tool runs and
answers `isError` with a plain sentence and no `kind`; every tool's parameters
reject an unknown field, so any of them can answer this way.

| `kind` | Source | Retry? |
|---|---|---|
| `api` | the API, with `status` and often `correlation_id` | 5xx and 429 with backoff; 4xx no |
| `transport` | connection, DNS or TLS | yes, with backoff |
| `auth` | no credentials, or credentials the token endpoint refused | no — configure credentials |
| `decode` | the API sent something the CLI cannot read, or — with the message `could not parse stdout as JSON` — this server could not read the CLI's stdout on a zero exit | no — surface it |
| `client` | bad arguments, a client-side refusal, or this server's write guard | no — fix the call |
| `spawn` | the OS could not start `flute2` | no — set `FLUTE2_BIN` |
| `timeout` | the child outran `FLUTE2_MCP_TIMEOUT_SECS` | depends on the tool's idempotency |
| `bad_output` | a non-zero exit with unparseable stdout; `stdout`/`stderr` truncated to 4 KiB with a `stdout_truncated`/`stderr_truncated` flag alongside | no — surface it |

## Security

**Card and bank values reach the CLI on its command line.** `flute2` takes the
card number, expiry, CVV, bank account number, routing number and tax id as
command-line arguments, so while a tool call runs those values are visible to
other processes on the host through process inspection (`ps`,
`/proc/<pid>/cmdline`). Run this server on a trusted host, under a dedicated
user, and do not pass real card or bank data on a shared or multi-tenant
machine. The argv interface is the CLI's, and no wrapper routes around it:
eliminating the exposure requires an upstream `flute2` change to accept these
values on stdin or in the environment rather than as flags.

This server reads and stores neither credential variable; both are inherited by
the child. A tool result can carry a credential, though: `api_keys_create`
forwards the API's response, which carries a `clientSecret` once and never
again, and `auth_status` returns `client_id`, never the secret. A transcript or
a ticket holding either result holds a credential.

Nothing `rmcp` emits below `info` is recorded, so the `tools/call` request and
its arguments stay out of the log whatever `RUST_LOG` asks for. This server logs
each call at `info` with the tool's name and whether it answered an error, and
with no argument.
`FLUTE2_MCP_DEBUG` copies the child's stderr into the log at debug level,
carrying whatever the CLI writes there; the default `info` filter drops it, so
it is recorded only when `RUST_LOG` admits this crate at `debug`.

The child's stderr is not returned on success. `bad_output` truncates both
streams to 4 KiB. Run production behind the write guard and lift it only for an
instance that is meant to move money.

## Licence

MIT.

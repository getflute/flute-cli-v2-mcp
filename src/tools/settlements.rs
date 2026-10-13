use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::{Id, PageArgs, opt, repeat};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettlementsList {
    /// Zero-based page index.
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub page_index: Option<u32>,
    /// Items per page, 1–100.
    #[schemars(range(min = 1, max = 100))]
    #[serde(default, deserialize_with = "crate::tools::de_page_size")]
    pub page_size: Option<u32>,
    /// Walk every page. Cannot be combined with `page_index`, and the response
    /// it returns carries no `meta.page_info`, so the per-page counts are absent.
    #[serde(default)]
    pub all: Option<bool>,
    /// `createdOn` (the default), `totalNetAmount`, `transactionCount` or
    /// `batchStatus`. With `sort_by` and no `asc`, results come back
    /// descending by that field. Any other spelling, a
    /// different case included, is refused by the CLI with `kind: "client"`.
    #[serde(default)]
    pub sort_by: Option<String>,
    /// Sort ascending. This group spells its reversal `asc`, not `desc`.
    #[serde(default)]
    pub asc: Option<bool>,
    /// Filter to batches created on or after this date, ISO 8601. A time is
    /// accepted but ignored: the filter starts at the beginning of that day.
    /// The API floors it at 60 days ago, so an earlier date returns the same
    /// 60-day window.
    #[serde(default)]
    pub from: Option<String>,
    /// Filter to batches created on or before this date, ISO 8601. A time is
    /// accepted but ignored: the filter includes that whole day. The API
    /// still refuses a `from` later than `to`, times included.
    #[serde(default)]
    pub to: Option<String>,
    /// Filter to these batch ids.
    #[serde(default)]
    pub batch_ids: Vec<String>,
    /// Filter to these processor ids.
    #[serde(default)]
    pub processor_ids: Vec<String>,
    /// `open`, `pending-settlement`, `settled` or `declined`.
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettlementClose {
    /// The processor whose open batch is closed.
    pub payment_processor_id: String,
    /// Sent as `Idempotency-Key`. Pass a fresh one, e.g. a UUID, on the
    /// first attempt; re-sending the same key with the same arguments returns
    /// the first result instead of repeating the operation. The same key with
    /// different arguments answers 422, and a key is not scoped to one tool,
    /// so use a new key for each distinct operation. At most 255 printable
    /// ASCII characters.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[tool_router(router = settlements_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(
        description = "List settlement batches. `page_index` is 0-based and `page_size` is 1–100; `all` walks every page and cannot be combined with `page_index`. Sorting reverses with `asc`, not `desc`. Safe to retry."
    )]
    pub async fn settlements_list(
        &self,
        Parameters(p): Parameters<SettlementsList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["settlements".into(), "list".into()]);
        PageArgs {
            page_index: p.page_index,
            page_size: p.page_size,
            all: p.all,
            sort_by: p.sort_by,
            asc: p.asc,
            desc: None,
        }
        .push(&mut args);
        opt(&mut args, "--from", p.from);
        opt(&mut args, "--to", p.to);
        repeat(&mut args, "--batch-ids", p.batch_ids);
        repeat(&mut args, "--processor-ids", p.processor_ids);
        opt(&mut args, "--status", p.status);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Get one settlement batch by its batch id — the list endpoint filtered server-side. Pass the batch id as `id`. A processor's open batch has `batchId: null` in settlements_list, so it cannot be read here; list with `status: \"open\"` instead. Safe to retry."
    )]
    pub async fn settlements_get(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["settlements".into(), "get".into(), p.id]);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Close a processor's open settlement batch. This closes the WHOLE open batch for that processor, not one transaction. Batch close is for card processors. After an ambiguous failure, re-send the same `idempotency_key` to get the first result rather than closing again."
    )]
    pub async fn settlements_close(
        &self,
        Parameters(p): Parameters<SettlementClose>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("settlements_close") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "settlements".into(),
            "close".into(),
            "--payment-processor-id".into(),
            p.payment_processor_id,
        ]);
        opt(&mut args, "--idempotency-key", p.idempotency_key);
        Ok(self.run_tool(args).await)
    }
}

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::{Id, PageArgs, opt};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TerminalsList {
    /// Zero-based page index.
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub page_index: Option<u32>,
    /// Items per page, 1–100.
    #[schemars(range(min = 1, max = 100))]
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub page_size: Option<u32>,
    /// Walk every page. Cannot be combined with `page_index`, and the response
    /// it returns carries no `meta.page_info`, so the per-page counts are absent.
    #[serde(default)]
    pub all: Option<bool>,
    /// One of `createdOn`, `serialNumber`, `terminalModel`,
    /// `terminalManufacturer`, `merchantCompanyName`. Any other spelling, a
    /// different case included, is refused by the CLI with `kind: "client"`.
    #[serde(default)]
    pub sort_by: Option<String>,
    /// Sort `sort_by` ascending. Cannot be combined with `desc`.
    #[serde(default)]
    pub asc: Option<bool>,
    /// Sort `sort_by` descending. Cannot be combined with `asc`.
    #[serde(default)]
    pub desc: Option<bool>,
    /// Readiness filter: `ready`, `busy` or `offline`. The response reports the same field in a different vocabulary, so a value read back is not necessarily one you can filter by.
    #[serde(default)]
    pub status: Option<String>,
    /// `standalone` or `semi-integrated`.
    #[serde(default)]
    pub mode: Option<String>,
    /// Connection status: `online` or `offline`.
    #[serde(default)]
    pub connection: Option<String>,
    /// Filter by serial number. An exact match.
    #[serde(default)]
    pub serial_number: Option<String>,
    /// Free-text search across serial number, terminal model, terminal
    /// manufacturer, terminal id and merchant company name.
    #[serde(default)]
    pub search: Option<String>,
}

#[tool_router(router = terminals_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(
        description = "List terminals. `page_index` is 0-based and `page_size` is 1–100; `all` walks every page and cannot be combined with `page_index`. This is where a `terminal_id` for pos_create comes from. Safe to retry."
    )]
    pub async fn terminals_list(
        &self,
        Parameters(p): Parameters<TerminalsList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["terminals".into(), "list".into()]);
        PageArgs {
            page_index: p.page_index,
            page_size: p.page_size,
            all: p.all,
            sort_by: p.sort_by,
            asc: p.asc,
            desc: p.desc,
        }
        .push(&mut args);
        opt(&mut args, "--status", p.status);
        opt(&mut args, "--mode", p.mode);
        opt(&mut args, "--connection", p.connection);
        opt(&mut args, "--serial-number", p.serial_number);
        opt(&mut args, "--search", p.search);
        Ok(self.run_tool(args).await)
    }

    #[tool(description = "Read one terminal's live status. Safe to retry.")]
    pub async fn terminals_status(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["terminals".into(), "status".into(), p.id]);
        Ok(self.run_tool(args).await)
    }
}

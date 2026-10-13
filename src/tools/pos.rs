use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::{Id, KeyedId, PageArgs, opt, switch};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PosCreate {
    /// The terminal to run the sale on, as a UUID. It has to be in
    /// semi-integrated mode and online and ready; terminals_list is where one
    /// comes from.
    pub terminal_id: String,
    /// The caller's own POS device identifier, at most 36 characters, e.g.
    /// `POS-DEVICE-001`.
    pub pos_device_id: String,
    /// Plain decimal, e.g. `10.50`.
    pub amount: String,
    /// Currency code, e.g. `USD`.
    pub currency_code: String,
    /// `auto` charges immediately, `manual` authorises for a later capture.
    /// The default is `auto`.
    #[serde(default)]
    pub capture_method: Option<String>,
    /// `cloud` (the default) pushes the sale over Flute's cloud connection
    /// and needs the terminal online and ready; `deeplink` has the caller
    /// launch the terminal app afterwards and checks no connectivity. A
    /// `deeplink` sale the terminal app never picks up is cancelled
    /// automatically a few minutes later.
    #[serde(default)]
    pub initiation_channel: Option<String>,
    /// `regular` (tap, insert or swipe) or `keyed-entry` (the card number
    /// keyed in on the terminal). Omit it to let the server decide.
    #[serde(default)]
    pub reading_method: Option<String>,
    /// `card` or `cash`. Required only where the merchant's zero-cost-processing
    /// option is dual pricing.
    #[serde(default)]
    pub pricing_type: Option<String>,
    /// Processor to charge through. Omit it for the merchant's default.
    #[serde(default)]
    pub payment_processor_id: Option<String>,
    /// Customer to associate the sale with.
    #[serde(default)]
    pub customer_id: Option<String>,
    /// Unique per merchant, forever and case-insensitively: a value already
    /// used answers 400 `ReferenceId already exists`, with no time window. That
    /// makes it a claim on a name rather than a retry guard — a create whose
    /// response is lost cannot be re-sent under the same reference.
    #[serde(default)]
    pub reference_id: Option<String>,
    /// Ask on the terminal whether to save the payment method. Only `true`
    /// has an effect here.
    #[serde(default)]
    pub request_storage_consent: Option<bool>,
    /// Tip as a plain decimal, e.g. `5.00`. At least 0.01, and mutually
    /// exclusive with `tip_rate`.
    #[serde(default)]
    pub tip_amount: Option<String>,
    /// Percentage, e.g. `18.5`.
    #[serde(default)]
    pub tip_rate: Option<String>,
    /// Sent as `Idempotency-Key`. Pass a fresh one, e.g. a UUID, on the
    /// first attempt; re-sending the same key with the same arguments returns
    /// the first result instead of repeating the operation. The same key with
    /// different arguments answers 422, and a key is not scoped to one tool,
    /// so use a new key for each distinct operation. At most 255 printable
    /// ASCII characters.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PosReversal {
    /// The terminal that handles the reversal, as a UUID.
    pub terminal_id: String,
    /// The caller's own POS device identifier, at most 36 characters.
    pub pos_device_id: String,
    /// Unique per merchant, the same way `pos_create`'s is: a value already
    /// used answers 400 `ReferenceId already exists`.
    pub reference_id: String,
    /// Transaction to reverse: the id transactions_get takes, which a POS
    /// transaction carries as `linkedTransaction.transactionId` — not the POS
    /// transaction id. Omit it for an unreferenced refund, which needs
    /// `amount` and `currency_code`.
    #[serde(default)]
    pub original_transaction_id: Option<String>,
    /// Plain decimal, e.g. `12.50`. Caps a refund of an original
    /// transaction; omit it to reverse the full amount. Ignored on a void.
    #[serde(default)]
    pub amount: Option<String>,
    /// Currency code, e.g. `USD`.
    #[serde(default)]
    pub currency_code: Option<String>,
    /// Processor to refund through. Omit it for the account's own.
    #[serde(default)]
    pub payment_processor_id: Option<String>,
    /// Customer to associate the reversal with.
    #[serde(default)]
    pub customer_id: Option<String>,
    /// `regular` or `keyed-entry`. Omit it to let the server decide.
    #[serde(default)]
    pub reading_method: Option<String>,
    /// `cloud` (the default) or `deeplink`.
    #[serde(default)]
    pub initiation_channel: Option<String>,
    /// Sent as `Idempotency-Key`. Pass a fresh one, e.g. a UUID, on the
    /// first attempt; re-sending the same key with the same arguments returns
    /// the first result instead of repeating the operation. The same key with
    /// different arguments answers 422, and a key is not scoped to one tool,
    /// so use a new key for each distinct operation. At most 255 printable
    /// ASCII characters.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PosList {
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
    /// `createdOn` (the default) or `posTransactionStatus`. Any other spelling, a
    /// different case included, is refused by the CLI with `kind: "client"`.
    #[serde(default)]
    pub sort_by: Option<String>,
    /// Sort `sort_by` ascending. Cannot be combined with `desc`.
    #[serde(default)]
    pub asc: Option<bool>,
    /// Sort `sort_by` descending. Cannot be combined with `asc`.
    #[serde(default)]
    pub desc: Option<bool>,
    /// Filter by terminal id.
    #[serde(default)]
    pub terminal_id: Option<String>,
    /// Filter to transactions created at or after this instant, inclusive.
    /// ISO 8601 UTC, e.g. `2026-01-01T00:00:00Z`.
    #[serde(default)]
    pub from: Option<String>,
    /// Filter to transactions created at or before this instant, inclusive.
    /// ISO 8601 UTC.
    #[serde(default)]
    pub to: Option<String>,
    /// `in-progress`, `completed`, `cancelled` or `failed`.
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrintReceipt {
    /// POS transaction id.
    pub id: String,
    /// Terminal to print on.
    pub terminal_id: String,
}

#[tool_router(router = pos_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(
        description = "Start a terminal sale. Returns the in-progress transaction immediately; poll pos_get until `data.posTransactionStatus` settles, then read `data.linkedTransaction.transactionStatus` to learn whether money moved: a sale the processor declined still settles as `posTransactionStatus: \"Completed\"`, and the exit code says neither. NOT safe to retry without `idempotency_key`: pass a fresh one, and after an ambiguous failure re-send the SAME key with the same arguments to get the first result back instead of a repeat. A replay returns the first response as it was, an error included, not the current state. Re-send the SAME `reference_id` as well — it is unique per merchant, so repeating it makes a keyless retry bounce, where varying it opens a second sale. Without a key, a second create against a terminal that already has one in progress is a real hazard, so reconcile with pos_get or pos_list first."
    )]
    pub async fn pos_create(
        &self,
        Parameters(p): Parameters<PosCreate>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("pos_create") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "pos".into(),
            "create".into(),
            "--terminal-id".into(),
            p.terminal_id,
            "--pos-device-id".into(),
            p.pos_device_id,
            "--amount".into(),
            p.amount,
            "--currency-code".into(),
            p.currency_code,
        ]);
        opt(&mut args, "--capture-method", p.capture_method);
        opt(&mut args, "--initiation-channel", p.initiation_channel);
        opt(&mut args, "--reading-method", p.reading_method);
        opt(&mut args, "--pricing-type", p.pricing_type);
        opt(&mut args, "--payment-processor-id", p.payment_processor_id);
        opt(&mut args, "--customer-id", p.customer_id);
        opt(&mut args, "--reference-id", p.reference_id);
        switch(
            &mut args,
            "--request-storage-consent",
            p.request_storage_consent,
        );
        opt(&mut args, "--tip-amount", p.tip_amount);
        opt(&mut args, "--tip-rate", p.tip_rate);
        opt(&mut args, "--idempotency-key", p.idempotency_key);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Reverse a POS transaction on a terminal, or refund with no original transaction. A void or a referenced credit-card refund completes on the API; a debit-card referenced refund or an unreferenced one waits for the card on the terminal, so poll pos_get on the returned POS transaction until `data.posTransactionStatus` settles, then read `data.linkedTransaction.transactionStatus` for the outcome. Moves money OUT. NOT safe to retry without `idempotency_key`: pass a fresh one, and after an ambiguous failure re-send the SAME key with the same arguments to get the first result back instead of a repeat. A replay returns the first response as it was, an error included, not the current state. Re-send the SAME `reference_id` as well — it is unique per merchant, so repeating it makes a keyless retry bounce, where varying it opens a second reversal. Without a key, reconcile with pos_get or pos_list first."
    )]
    pub async fn pos_reversal(
        &self,
        Parameters(p): Parameters<PosReversal>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("pos_reversal") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "pos".into(),
            "reversal".into(),
            "--terminal-id".into(),
            p.terminal_id,
            "--pos-device-id".into(),
            p.pos_device_id,
            "--reference-id".into(),
            p.reference_id,
        ]);
        opt(
            &mut args,
            "--original-transaction-id",
            p.original_transaction_id,
        );
        opt(&mut args, "--amount", p.amount);
        opt(&mut args, "--currency-code", p.currency_code);
        opt(&mut args, "--payment-processor-id", p.payment_processor_id);
        opt(&mut args, "--customer-id", p.customer_id);
        opt(&mut args, "--reading-method", p.reading_method);
        opt(&mut args, "--initiation-channel", p.initiation_channel);
        opt(&mut args, "--idempotency-key", p.idempotency_key);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Get a POS transaction by id. This is the polling read after pos_create: `data.posTransactionStatus` says whether the terminal is done, and `data.linkedTransaction.transactionStatus` says whether money moved — `Completed` with a `Declined` linked transaction is a decline. Safe to retry."
    )]
    pub async fn pos_get(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["pos".into(), "get".into(), p.id]);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "List POS transactions. `page_index` is 0-based and `page_size` is 1–100; `all` walks every page and cannot be combined with `page_index`. Safe to retry."
    )]
    pub async fn pos_list(
        &self,
        Parameters(p): Parameters<PosList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["pos".into(), "list".into()]);
        PageArgs {
            page_index: p.page_index,
            page_size: p.page_size,
            all: p.all,
            sort_by: p.sort_by,
            asc: p.asc,
            desc: p.desc,
        }
        .push(&mut args);
        opt(&mut args, "--terminal-id", p.terminal_id);
        opt(&mut args, "--from", p.from);
        opt(&mut args, "--to", p.to);
        opt(&mut args, "--status", p.status);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Cancel a POS transaction. A success carries the API's own body. A cancelled transaction still exists, so a second cancel without a key answers 400; re-sending the first cancel's `idempotency_key` returns the first result instead. A 404 is an id the server never had. Without a key, reconcile with pos_get before re-issuing."
    )]
    pub async fn pos_cancel(
        &self,
        Parameters(p): Parameters<KeyedId>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("pos_cancel") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend(["pos".into(), "cancel".into(), p.id, "--yes".into()]);
        opt(&mut args, "--idempotency-key", p.idempotency_key);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Print a receipt on a terminal. THIS PRINTS ON A PHYSICAL DEVICE. Only a POS transaction whose sale was approved prints; a declined one answers 400. The success is a confirmation carrying `posTransactionId` and `sent: true`, which says the API accepted the request, not that paper came out."
    )]
    pub async fn pos_print_receipt(
        &self,
        Parameters(p): Parameters<PrintReceipt>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("pos_print_receipt") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "pos".into(),
            "print-receipt".into(),
            p.id,
            "--terminal-id".into(),
            p.terminal_id,
        ]);
        Ok(self.run_tool(args).await)
    }
}

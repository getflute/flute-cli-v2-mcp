use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::{Id, PageArgs, opt, repeat, switch, toggle};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PaymentLinkCreate {
    /// Currency code, e.g. `USD`. Required.
    pub currency_code: String,
    /// Accept card payments. At least one payment method is required.
    #[serde(default)]
    pub card_enabled: Option<bool>,
    /// Processor charging the link's card payments; it has to be an active
    /// card processor on the account. Omit it to pin the account's current
    /// default.
    #[serde(default)]
    pub card_processor_id: Option<String>,
    /// Accept ACH payments. At least one payment method is required.
    #[serde(default)]
    pub ach_enabled: Option<bool>,
    /// Processor charging the link's ACH payments; it has to be an active
    /// ACH processor on the account. Omit it to pin the account's current
    /// default.
    #[serde(default)]
    pub ach_processor_id: Option<String>,
    /// Plain decimal. Omit for a link the payer fills in.
    #[serde(default)]
    pub amount: Option<String>,
    /// `single-use` (default) or `multi-use`.
    #[serde(default)]
    pub link_type: Option<String>,
    /// Customer the link is issued for. Omit it for an anonymous link.
    #[serde(default)]
    pub customer_id: Option<String>,
    /// Your own correlation string. It does not dedupe: two creates carrying
    /// the same value produce two independent links with different short urls.
    #[serde(default)]
    pub reference_id: Option<String>,
    /// Merchant-facing label, at most 80 characters. Generated from the
    /// amount when omitted.
    #[serde(default)]
    pub name: Option<String>,
    /// Merchant-internal notes, never shown to customers.
    #[serde(default)]
    pub description: Option<String>,
    /// UTC expiry, ISO 8601 ending in `Z`, e.g. `2030-09-15T00:00:00Z`. Omit
    /// for a link that never expires.
    #[serde(default)]
    pub expires_on: Option<String>,
    /// What the link does with the payer's payment method: `payment`
    /// (default) or `payment-and-save`. Fixed at create.
    #[serde(default)]
    pub mode: Option<String>,
    /// Arbitrary `key=value` pairs handed back on the read.
    #[serde(default)]
    pub metadata: Vec<String>,
    /// Tax as a plain amount, e.g. `1.00`. The API requires `tax_mode` with
    /// exactly one of `tax_rate` or `tax_amount`.
    #[serde(default)]
    pub tax_amount: Option<String>,
    /// Tax as a percentage, e.g. `8.5` for 8.5%. The API requires `tax_mode`
    /// with exactly one of `tax_rate` or `tax_amount`.
    #[serde(default)]
    pub tax_rate: Option<String>,
    /// `exclusive` (tax added to the amount) or `inclusive` (tax included in
    /// it). Sent with exactly one of `tax_rate` or `tax_amount`.
    #[serde(default)]
    pub tax_mode: Option<String>,
    /// Ask for faster ACH processing. Requires `ach_enabled`, and the
    /// merchant's ACH settings must allow it.
    #[serde(default)]
    pub ach_faster_processing: Option<bool>,
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
pub struct PaymentLinksList {
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
    /// `createdOn` (the default), `baseAmount`, `name` or
    /// `paymentLinkStatus`. With `sort_by` and neither `asc` nor `desc`, results come back
    /// descending by that field. Any other spelling, a
    /// different case included, is refused by the CLI with `kind: "client"`.
    #[serde(default)]
    pub sort_by: Option<String>,
    /// Sort `sort_by` ascending. Cannot be combined with `desc`.
    #[serde(default)]
    pub asc: Option<bool>,
    /// Sort `sort_by` descending. Cannot be combined with `asc`.
    #[serde(default)]
    pub desc: Option<bool>,
    /// Free-text match against the link's name, description and id.
    #[serde(default)]
    pub search: Option<String>,
    /// `single-use` or `multi-use`.
    #[serde(default)]
    pub link_type: Option<String>,
    /// `active`, `completed`, `expired` or `inactive`.
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PaymentLinkUpdate {
    /// Payment link id to update.
    pub id: String,
    /// Accept card payments. `false` stops it.
    #[serde(default)]
    pub card_enabled: Option<bool>,
    /// Processor charging the link's card payments; it has to be an active
    /// card processor on the account. Omit it along with `card_enabled` to
    /// leave the link's card configuration untouched; setting
    /// `card_enabled` without it pins the account's current default.
    #[serde(default)]
    pub card_processor_id: Option<String>,
    /// Accept ACH payments. `false` stops it, and the link has to keep at
    /// least one method enabled.
    #[serde(default)]
    pub ach_enabled: Option<bool>,
    /// Processor charging the link's ACH payments; it has to be an active
    /// ACH processor on the account. Omit it along with `ach_enabled` to
    /// leave the link's ACH configuration untouched; setting `ach_enabled`
    /// without it pins the account's current default.
    #[serde(default)]
    pub ach_processor_id: Option<String>,
    /// New amount to collect, a plain decimal greater than zero. Clear it
    /// with `clear: ["amount"]` for a link the payer fills in.
    #[serde(default)]
    pub amount: Option<String>,
    /// Frozen once the link has taken a payment.
    #[serde(default)]
    pub currency_code: Option<String>,
    /// `single-use` or `multi-use`, evaluated per payment at submit time. It
    /// cannot be cleared.
    #[serde(default)]
    pub link_type: Option<String>,
    /// `active` or `inactive`; `inactive` stops accepting payments.
    #[serde(default)]
    pub status: Option<String>,
    /// Customer the link is issued for. Clearing it detaches the customer.
    #[serde(default)]
    pub customer_id: Option<String>,
    /// New merchant-assigned reference, at most 200 characters. It does not
    /// dedupe.
    #[serde(default)]
    pub reference_id: Option<String>,
    /// New merchant-facing label, at most 80 characters. It cannot be
    /// cleared.
    #[serde(default)]
    pub name: Option<String>,
    /// New merchant-internal notes, at most 500 characters. Never shown to
    /// customers.
    #[serde(default)]
    pub description: Option<String>,
    /// New UTC expiry, ISO 8601 ending in `Z`, e.g. `2030-09-15T00:00:00Z`.
    /// It must be in the future.
    #[serde(default)]
    pub expires_on: Option<String>,
    /// `key=value` pairs merged into the stored map: a named key is added
    /// or replaced and the other keys are kept. An empty value after `=`
    /// stores an empty string. `clear: ["metadata"]` removes the whole map.
    #[serde(default)]
    pub metadata: Vec<String>,
    /// New tax as a plain amount, e.g. `1.00`. Needs a `tax_mode`, sent here
    /// or already stored. A link that holds a `tax_rate` refuses it unless
    /// the same call carries `clear: ["tax-rate"]`.
    #[serde(default)]
    pub tax_amount: Option<String>,
    /// New tax as a percentage, e.g. `8.5` for 8.5%. Needs a `tax_mode`,
    /// sent here or already stored. A link that holds a `tax_amount` refuses
    /// it unless the same call carries `clear: ["tax-amount"]`.
    #[serde(default)]
    pub tax_rate: Option<String>,
    /// `exclusive` or `inclusive`. Sent with exactly one of `tax_rate` or
    /// `tax_amount`, and cleared with them.
    #[serde(default)]
    pub tax_mode: Option<String>,
    /// Ask for faster ACH processing, or `false` to stop asking. Requires
    /// `ach_enabled: true`, and the merchant's ACH settings must allow it.
    #[serde(default)]
    pub ach_faster_processing: Option<bool>,
    /// Fields to clear outright: `amount`, `customer-id`, `reference-id`,
    /// `description`, `expires-on`, `tax-amount`, `tax-rate`, `tax-mode`,
    /// `metadata`. `name`, `currency-code` and the processor ids cannot be
    /// cleared.
    #[serde(default)]
    pub clear: Vec<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShareLink {
    /// Payment link id.
    pub id: String,
    /// `email` or `sms`.
    pub share_by: String,
    /// Email address or mobile number in E.164 form.
    pub recipient: String,
    /// The customer has consented to receive it. The API rejects a share without it.
    #[serde(default)]
    pub consent: Option<bool>,
    /// Sent as `Idempotency-Key`. Pass a fresh one, e.g. a UUID, on the
    /// first attempt; re-sending the same key with the same arguments returns
    /// the first result instead of repeating the operation. The same key with
    /// different arguments answers 422, and a key is not scoped to one tool,
    /// so use a new key for each distinct operation. At most 255 printable
    /// ASCII characters.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[tool_router(router = payment_links_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "Get a payment link by id. Safe to retry.")]
    pub async fn payment_links_get(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["payment-links".into(), "get".into(), p.id]);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "List payment links. `page_index` is 0-based and `page_size` is 1–100; `all` walks every page and cannot be combined with `page_index`. Safe to retry."
    )]
    pub async fn payment_links_list(
        &self,
        Parameters(p): Parameters<PaymentLinksList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["payment-links".into(), "list".into()]);
        PageArgs {
            page_index: p.page_index,
            page_size: p.page_size,
            all: p.all,
            sort_by: p.sort_by,
            asc: p.asc,
            desc: p.desc,
        }
        .push(&mut args);
        opt(&mut args, "--search", p.search);
        opt(&mut args, "--link-type", p.link_type);
        opt(&mut args, "--status", p.status);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Create a payment link. At least one of `card_enabled` / `ach_enabled` is required. NOT safe to retry without `idempotency_key`: pass a fresh one, and after an ambiguous failure re-send the SAME key with the same arguments to get the first result back instead of a repeat. A replay returns the first response as it was, an error included, not the current state. Nothing else refuses a repeat: a second create without the key answers a second live link. Without a key, reconcile with payment_links_list before re-issuing, which filters on `search` rather than on `reference_id`."
    )]
    pub async fn payment_links_create(
        &self,
        Parameters(p): Parameters<PaymentLinkCreate>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("payment_links_create") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "payment-links".into(),
            "create".into(),
            "--currency-code".into(),
            p.currency_code,
        ]);
        switch(&mut args, "--card-enabled", p.card_enabled);
        opt(&mut args, "--card-processor-id", p.card_processor_id);
        switch(&mut args, "--ach-enabled", p.ach_enabled);
        opt(&mut args, "--ach-processor-id", p.ach_processor_id);
        opt(&mut args, "--amount", p.amount);
        opt(&mut args, "--link-type", p.link_type);
        opt(&mut args, "--customer-id", p.customer_id);
        opt(&mut args, "--reference-id", p.reference_id);
        opt(&mut args, "--name", p.name);
        opt(&mut args, "--description", p.description);
        opt(&mut args, "--expires-on", p.expires_on);
        opt(&mut args, "--mode", p.mode);
        repeat(&mut args, "--metadata", p.metadata);
        opt(&mut args, "--tax-amount", p.tax_amount);
        opt(&mut args, "--tax-rate", p.tax_rate);
        opt(&mut args, "--tax-mode", p.tax_mode);
        switch(
            &mut args,
            "--ach-faster-processing",
            p.ach_faster_processing,
        );
        opt(&mut args, "--idempotency-key", p.idempotency_key);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Update a payment link. Omitted fields keep their stored values; `clear` removes a value outright. Safe to retry."
    )]
    pub async fn payment_links_update(
        &self,
        Parameters(p): Parameters<PaymentLinkUpdate>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("payment_links_update") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend(["payment-links".into(), "update".into(), p.id]);
        toggle(&mut args, "--card-enabled", p.card_enabled);
        opt(&mut args, "--card-processor-id", p.card_processor_id);
        toggle(&mut args, "--ach-enabled", p.ach_enabled);
        opt(&mut args, "--ach-processor-id", p.ach_processor_id);
        opt(&mut args, "--amount", p.amount);
        opt(&mut args, "--currency-code", p.currency_code);
        opt(&mut args, "--link-type", p.link_type);
        opt(&mut args, "--status", p.status);
        opt(&mut args, "--customer-id", p.customer_id);
        opt(&mut args, "--reference-id", p.reference_id);
        opt(&mut args, "--name", p.name);
        opt(&mut args, "--description", p.description);
        opt(&mut args, "--expires-on", p.expires_on);
        repeat(&mut args, "--metadata", p.metadata);
        opt(&mut args, "--tax-amount", p.tax_amount);
        opt(&mut args, "--tax-rate", p.tax_rate);
        opt(&mut args, "--tax-mode", p.tax_mode);
        toggle(
            &mut args,
            "--ach-faster-processing",
            p.ach_faster_processing,
        );
        repeat(&mut args, "--clear", p.clear);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Delete a payment link. A 404 is a success, so re-running is a safe no-op, but its confirmation carries `deleted: false, found: false`: the server answers alike for an id already deleted and one that never existed. `deleted: true` means this call removed the payment link."
    )]
    pub async fn payment_links_delete(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("payment_links_delete") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "payment-links".into(),
            "delete".into(),
            p.id,
            "--yes".into(),
        ]);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Send a payment link to a customer. THIS SENDS A REAL MESSAGE. `consent` is required on every channel, email included. Re-sending with the same `idempotency_key` and arguments returns the first result instead of sending again. The success is a confirmation carrying `paymentLinkId` and `shared: true`, which reports acceptance and not delivery — an undeliverable address still answers `shared: true`."
    )]
    pub async fn payment_links_share(
        &self,
        Parameters(p): Parameters<ShareLink>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("payment_links_share") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "payment-links".into(),
            "share".into(),
            p.id,
            "--share-by".into(),
            p.share_by,
            "--recipient".into(),
            p.recipient,
        ]);
        switch(&mut args, "--consent", p.consent);
        opt(&mut args, "--idempotency-key", p.idempotency_key);
        Ok(self.run_tool(args).await)
    }
}

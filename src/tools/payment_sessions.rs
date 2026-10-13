use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::{Id, KeyedId, opt, repeat, switch};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PaymentSessionCreate {
    /// `payment` (default), `save-method` or `payment-and-save`.
    #[serde(default)]
    pub mode: Option<String>,
    /// Plain decimal. Omit for a session the payer fills in; a `save-method`
    /// session needs none.
    #[serde(default)]
    pub amount: Option<String>,
    /// Tip as a plain decimal, charged on top of `amount`. Refused on an
    /// account with tips disabled; settings_payment_config shows the
    /// setting.
    #[serde(default)]
    pub tip_amount: Option<String>,
    /// Customer to attach the vaulted payment method to. It cannot be
    /// combined with `customer_handling: "token-only"`.
    #[serde(default)]
    pub customer_id: Option<String>,
    /// `create-customer` or `token-only`.
    #[serde(default)]
    pub customer_handling: Option<String>,
    /// Your own correlation string, echoed back on read. It does not dedupe:
    /// two creates carrying the same value produce two independent sessions.
    #[serde(default)]
    pub reference_id: Option<String>,
    /// URL to send the payer to after a successful payment.
    #[serde(default)]
    pub return_url: Option<String>,
    /// Bypass address verification. Only meaningful on a paying session.
    #[serde(default)]
    pub skip_address_verification: Option<bool>,
    /// Display name shown on the checkout page.
    #[serde(default)]
    pub page_name: Option<String>,
    /// Notes shown to the payer on the checkout page.
    #[serde(default)]
    pub payment_notes: Option<String>,
    /// Message shown to the payer once the session completes.
    #[serde(default)]
    pub after_completion_message: Option<String>,
    /// UTC expiry, ISO 8601 ending in `Z`, e.g. `2030-09-15T00:00:00Z`. Omit
    /// for a session that never expires.
    #[serde(default)]
    pub expires_at: Option<String>,
    /// Arbitrary `key=value` pairs handed back on the read.
    #[serde(default)]
    pub metadata: Vec<String>,
    /// Offer card at checkout. Omit both `card_enabled` and `ach_enabled` to
    /// offer every method the account has an active processor for; the
    /// session then reads back `paymentMethods: null`.
    #[serde(default)]
    pub card_enabled: Option<bool>,
    /// Processor charging the session's card payments; it has to be an
    /// active card processor on the account. Omit it to pin the account's
    /// current default.
    #[serde(default)]
    pub card_processor_id: Option<String>,
    /// Offer ACH at checkout. Omit both `card_enabled` and `ach_enabled` to
    /// offer every method the account has an active processor for; the
    /// session then reads back `paymentMethods: null`.
    #[serde(default)]
    pub ach_enabled: Option<bool>,
    /// Processor charging the session's ACH payments; it has to be an active
    /// ACH processor on the account. Omit it to pin the account's current
    /// default.
    #[serde(default)]
    pub ach_processor_id: Option<String>,
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

#[tool_router(router = payment_sessions_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(
        description = "Get a payment session by id. The read response carries no id of any spelling, so it cannot be correlated back to the session that was read — track the id from the create. It carries no amount field either, only the tip, tax and surcharge amounts. Safe to retry."
    )]
    pub async fn payment_sessions_get(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["payment-sessions".into(), "get".into(), p.id]);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Create a hosted-checkout payment session. NOT safe to retry without `idempotency_key`: pass a fresh one, and after an ambiguous failure re-send the SAME key with the same arguments to get the first result back instead of a repeat. A replay returns the first response as it was, an error included, not the current state. Without a key, a create whose response is lost cannot be reconciled: this class carries no list verb, the id it returned is the only handle on the session, and `reference_id` neither dedupes nor is queryable."
    )]
    pub async fn payment_sessions_create(
        &self,
        Parameters(p): Parameters<PaymentSessionCreate>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("payment_sessions_create") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend(["payment-sessions".into(), "create".into()]);
        opt(&mut args, "--mode", p.mode);
        opt(&mut args, "--amount", p.amount);
        opt(&mut args, "--tip-amount", p.tip_amount);
        opt(&mut args, "--customer-id", p.customer_id);
        opt(&mut args, "--customer-handling", p.customer_handling);
        opt(&mut args, "--reference-id", p.reference_id);
        opt(&mut args, "--return-url", p.return_url);
        switch(
            &mut args,
            "--skip-address-verification",
            p.skip_address_verification,
        );
        opt(&mut args, "--page-name", p.page_name);
        opt(&mut args, "--payment-notes", p.payment_notes);
        opt(
            &mut args,
            "--after-completion-message",
            p.after_completion_message,
        );
        opt(&mut args, "--expires-at", p.expires_at);
        repeat(&mut args, "--metadata", p.metadata);
        switch(&mut args, "--card-enabled", p.card_enabled);
        opt(&mut args, "--card-processor-id", p.card_processor_id);
        switch(&mut args, "--ach-enabled", p.ach_enabled);
        opt(&mut args, "--ach-processor-id", p.ach_processor_id);
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
        description = "Cancel a payment session. The success is a confirmation carrying `id` and `cancelled: true`. A cancelled session still exists, so a second cancel without a key answers 400; re-sending the first cancel's `idempotency_key` returns the first result instead. A 404 is an id the server never had. Without a key, reconcile with payment_sessions_get before re-issuing."
    )]
    pub async fn payment_sessions_cancel(
        &self,
        Parameters(p): Parameters<KeyedId>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("payment_sessions_cancel") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "payment-sessions".into(),
            "cancel".into(),
            p.id,
            "--yes".into(),
        ]);
        opt(&mut args, "--idempotency-key", p.idempotency_key);
        Ok(self.run_tool(args).await)
    }
}

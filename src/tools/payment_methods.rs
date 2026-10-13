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
pub struct PaymentMethodsList {
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
    /// `createdOn` or `name` (the method's label). With no `sort_by`, `asc`
    /// or `desc`, the API applies no documented order; with `sort_by: "name"`
    /// alone, results come back ascending. Any other spelling, a
    /// different case included, is refused by the CLI with `kind: "client"`.
    #[serde(default)]
    pub sort_by: Option<String>,
    /// Sort ascending, by `sort_by` or else by creation date. Cannot be
    /// combined with `desc`.
    #[serde(default)]
    pub asc: Option<bool>,
    /// Sort descending, by `sort_by` or else by creation date. Cannot be
    /// combined with `asc`.
    #[serde(default)]
    pub desc: Option<bool>,
    /// Match methods whose label (the `name` they were given) contains this
    /// text, case-sensitively.
    #[serde(default)]
    pub search: Option<String>,
    /// Filter to methods vaulted against this customer.
    #[serde(default)]
    pub customer_id: Option<String>,
    /// Filter to methods created on or after this date, ISO 8601. A time is
    /// accepted but ignored: the filter starts at the beginning of that day.
    #[serde(default)]
    pub created_from: Option<String>,
    /// Filter to methods created on or before this date, ISO 8601. A time is
    /// accepted but ignored: the filter includes that whole day.
    #[serde(default)]
    pub created_to: Option<String>,
}

#[derive(Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddCard {
    /// Card PAN, e.g. `4111111111111111`.
    pub card: String,
    /// Expiry in `MM/YY` or `MM/YYYY`.
    pub exp: String,
    /// Card security code, three or four digits.
    #[serde(default)]
    pub cvv: Option<String>,
    /// Customer to vault the card against. Omit for an unattached method.
    #[serde(default)]
    pub customer_id: Option<String>,
    /// Friendly label for the stored method, e.g. `My Visa Card`.
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddAch {
    /// Bank account number.
    pub account: String,
    /// Routing number (ABA).
    pub routing: String,
    /// `business` or `personal`.
    pub account_holder_type: String,
    /// `checking` or `savings`.
    #[serde(default)]
    pub account_type: Option<String>,
    /// Tax ID of the account holder, e.g. `12-3456789`.
    #[serde(default)]
    pub tax_id: Option<String>,
    /// Customer to vault the account against. Omit it for a method that
    /// belongs to the merchant only.
    #[serde(default)]
    pub customer_id: Option<String>,
    /// Friendly label for the stored method, e.g. `Business Checking`.
    #[serde(default)]
    pub name: Option<String>,
    /// Legal entity name. Required for an unattached business ACH method
    /// where the transaction's own contact info does not carry one. With
    /// `customer_id` the customer's company name is used instead and this
    /// value is not stored, so a business account needs a company name on
    /// the customer.
    #[serde(default)]
    pub company_name: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PaymentMethodUpdate {
    /// Payment method id to rename.
    pub id: String,
    /// The new display name, at most 20 characters. It is the only updatable
    /// field, so an update without it would spend a round trip changing nothing.
    pub name: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetDefault {
    /// Payment method to make the default for its own instrument type.
    pub id: String,
    /// Customer the default applies to.
    pub customer_id: String,
}

#[tool_router(router = payment_methods_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "Get a stored payment method by id. Safe to retry.")]
    pub async fn payment_methods_get(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["payment-methods".into(), "get".into(), p.id]);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "List stored payment methods. `page_index` is 0-based and `page_size` is 1–100; `all` walks every page and cannot be combined with `page_index`. Safe to retry."
    )]
    pub async fn payment_methods_list(
        &self,
        Parameters(p): Parameters<PaymentMethodsList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["payment-methods".into(), "list".into()]);
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
        opt(&mut args, "--customer-id", p.customer_id);
        opt(&mut args, "--created-from", p.created_from);
        opt(&mut args, "--created-to", p.created_to);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Vault a card. NOT idempotent and it takes no reference id, so a repeat stores a second method — reconcile with payment_methods_list before re-issuing after an ambiguous failure."
    )]
    pub async fn payment_methods_add_card(
        &self,
        Parameters(p): Parameters<AddCard>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("payment_methods_add_card") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "payment-methods".into(),
            "add-card".into(),
            "--card".into(),
            p.card,
            "--exp".into(),
            p.exp,
        ]);
        opt(&mut args, "--cvv", p.cvv);
        opt(&mut args, "--customer-id", p.customer_id);
        opt(&mut args, "--name", p.name);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Vault a bank account for ACH. NOT idempotent and it takes no reference id — reconcile with payment_methods_list before re-issuing after an ambiguous failure."
    )]
    pub async fn payment_methods_add_ach(
        &self,
        Parameters(p): Parameters<AddAch>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("payment_methods_add_ach") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "payment-methods".into(),
            "add-ach".into(),
            "--account".into(),
            p.account,
            "--routing".into(),
            p.routing,
            "--account-holder-type".into(),
            p.account_holder_type,
        ]);
        opt(&mut args, "--account-type", p.account_type);
        opt(&mut args, "--tax-id", p.tax_id);
        opt(&mut args, "--customer-id", p.customer_id);
        opt(&mut args, "--name", p.name);
        opt(&mut args, "--company-name", p.company_name);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Rename a stored payment method. The success is a confirmation carrying `paymentMethodId` and `updated: true`, not the updated record. Safe to retry."
    )]
    pub async fn payment_methods_update(
        &self,
        Parameters(p): Parameters<PaymentMethodUpdate>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("payment_methods_update") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "payment-methods".into(),
            "update".into(),
            p.id,
            "--name".into(),
            p.name,
        ]);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Delete a stored payment method. A 404 is a success, so re-running is a safe no-op, but its confirmation carries `deleted: false, found: false`: the server answers alike for an id already deleted and one that never existed. `deleted: true` means this call removed the payment method."
    )]
    pub async fn payment_methods_delete(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("payment_methods_delete") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "payment-methods".into(),
            "delete".into(),
            p.id,
            "--yes".into(),
        ]);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Make a stored payment method the customer's default for its own instrument type. A default card and a default ACH account coexist — setting one clears the previous default of that type and leaves the other alone, and `customers_get` reports the two in separate `cards` and `achAccounts` arrays. The success is a confirmation carrying `paymentMethodId` and `default: true`. Safe to retry."
    )]
    pub async fn payment_methods_set_default(
        &self,
        Parameters(p): Parameters<SetDefault>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("payment_methods_set_default") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "payment-methods".into(),
            "set-default".into(),
            p.id,
            "--customer-id".into(),
            p.customer_id,
        ]);
        Ok(self.run_tool(args).await)
    }
}

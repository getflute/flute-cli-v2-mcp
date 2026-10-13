use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::{Empty, opt};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateAutofill {
    /// Default level-2 sales tax rate as a percentage, 0 to 22. It cannot be
    /// cleared: an empty string is refused with `kind: "client"`.
    #[serde(default)]
    pub l2_tax_rate: Option<String>,
    /// Default level-3 shipping charge rate, as a percentage.
    #[serde(default)]
    pub l3_shipping_rate: Option<String>,
    /// Default level-3 duty charge rate, as a percentage.
    #[serde(default)]
    pub l3_duty_rate: Option<String>,
    /// Default product name for autofilled line items, e.g. `Widget`.
    #[serde(default)]
    pub product_name: Option<String>,
    /// Default merchant-assigned product code, e.g. `SKU-12345`.
    #[serde(default)]
    pub product_code: Option<String>,
    /// Unit of measurement, e.g. `EA`.
    #[serde(default)]
    pub product_unit: Option<String>,
    /// Default unit price as a plain decimal, e.g. `19.99`.
    #[serde(default)]
    pub product_unit_price: Option<String>,
    /// Default quantity, e.g. `1`.
    #[serde(default)]
    pub product_quantity: Option<String>,
    /// Default product discount, as a percentage.
    #[serde(default)]
    pub product_discount: Option<String>,
}

#[tool_router(router = settings_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(
        description = "Read the account's payment configuration. This is where processor ids come from, and there is one per payment method — sending an ACH debit to a card processor fails in a way that looks nothing like the cause. Safe to retry."
    )]
    pub async fn settings_payment_config(
        &self,
        _p: Parameters<Empty>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["settings".into(), "payment-config".into()]);
        Ok(self.run_tool(args).await)
    }

    #[tool(description = "Read the account's contact information. Safe to retry.")]
    pub async fn settings_contact_info(
        &self,
        _p: Parameters<Empty>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["settings".into(), "contact-info".into()]);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Read the level 2 and level 3 autofill defaults. The response names no resource, so there is no id to correlate it by. Safe to retry."
    )]
    pub async fn settings_autofill(
        &self,
        _p: Parameters<Empty>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["settings".into(), "autofill".into()]);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Update the level 2 and level 3 autofill defaults. Omitted fields keep their stored values, so a partial update leaves its siblings alone. An empty string clears that field, except `l2_tax_rate`, which cannot be cleared. The success is the full autofill record. Safe to retry."
    )]
    pub async fn settings_update_autofill(
        &self,
        Parameters(p): Parameters<UpdateAutofill>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("settings_update_autofill") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend(["settings".into(), "update-autofill".into()]);
        opt(&mut args, "--l2-tax-rate", p.l2_tax_rate);
        opt(&mut args, "--l3-shipping-rate", p.l3_shipping_rate);
        opt(&mut args, "--l3-duty-rate", p.l3_duty_rate);
        opt(&mut args, "--product-name", p.product_name);
        opt(&mut args, "--product-code", p.product_code);
        opt(&mut args, "--product-unit", p.product_unit);
        opt(&mut args, "--product-unit-price", p.product_unit_price);
        opt(&mut args, "--product-quantity", p.product_quantity);
        opt(&mut args, "--product-discount", p.product_discount);
        Ok(self.run_tool(args).await)
    }
}

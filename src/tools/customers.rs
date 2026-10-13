use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::error::FluteError;
use crate::server::FluteServer;
use crate::tools::address::AddressArgs;
use crate::tools::{Id, PageArgs, flute_err_to_result, opt, switch, toggle};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomersList {
    /// Zero-based page index: 0 (or omit) is the first page.
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
    /// One of `companyName`, `createdOn`, `email`, `firstName`, `lastName`,
    /// `lastTransactionDate`, `mobilePhoneNumber` or `paymentMethodsCount`.
    /// Any other spelling, a
    /// different case included, is refused by the CLI with `kind: "client"`. With
    /// neither `sort_by`, `asc` nor `desc`, results come back newest first;
    /// with `sort_by` alone, descending by that field.
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
    /// Filter by full name.
    #[serde(default)]
    pub full_name: Option<String>,
    /// Filter by email address.
    #[serde(default)]
    pub email: Option<String>,
    /// Filter by company name.
    #[serde(default)]
    pub company_name: Option<String>,
    /// Filter by mobile number. A free-form search, so partial digits and
    /// legacy formats match; E.164 is not required here.
    #[serde(default)]
    pub mobile: Option<String>,
    /// Creation-date lower bound, ISO 8601. The API accepts it but does not
    /// apply it: every customer comes back whatever the value.
    #[serde(default)]
    pub created_from: Option<String>,
    /// Creation-date upper bound, ISO 8601. The API accepts it but does not
    /// apply it: every customer comes back whatever the value.
    #[serde(default)]
    pub created_to: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomerCreate {
    /// Customer first name.
    pub first_name: String,
    /// Customer last name.
    pub last_name: String,
    /// Customer company name.
    #[serde(default)]
    pub company: Option<String>,
    /// Customer email address.
    #[serde(default)]
    pub email: Option<String>,
    /// Mobile number in E.164 form, e.g. `+14155552309`.
    #[serde(default)]
    pub mobile: Option<String>,
    /// Record consent to receive SMS. Only `true` has an effect here.
    #[serde(default)]
    pub sms_consent: Option<bool>,
    /// Reuse the billing address as the shipping address. Only `true` has an
    /// effect here.
    #[serde(default)]
    pub use_billing_as_shipping: Option<bool>,
    /// AVS billing street line 1.
    #[serde(default)]
    pub billing_line1: Option<String>,
    /// AVS billing street line 2.
    #[serde(default)]
    pub billing_line2: Option<String>,
    /// AVS billing city.
    #[serde(default)]
    pub billing_city: Option<String>,
    /// AVS billing state code, e.g. `CO`.
    #[serde(default)]
    pub billing_state: Option<String>,
    /// AVS billing postal code.
    #[serde(default)]
    pub billing_postal_code: Option<String>,
    /// AVS billing country code, e.g. `US`.
    #[serde(default)]
    pub billing_country: Option<String>,
    /// Shipping street line 1.
    #[serde(default)]
    pub shipping_line1: Option<String>,
    /// Shipping street line 2.
    #[serde(default)]
    pub shipping_line2: Option<String>,
    /// Shipping city.
    #[serde(default)]
    pub shipping_city: Option<String>,
    /// Shipping state code, e.g. `CO`. The ISO 3166-2 subdivision code, which
    /// for a US state is the two-letter USPS abbreviation.
    #[serde(default)]
    pub shipping_state: Option<String>,
    /// Shipping postal code.
    #[serde(default)]
    pub shipping_postal_code: Option<String>,
    /// Shipping country code, e.g. `US`. ISO 3166-1 alpha-2.
    #[serde(default)]
    pub shipping_country: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomerUpdate {
    /// Customer id to update.
    pub id: String,
    /// New first name. It cannot be cleared.
    #[serde(default)]
    pub first_name: Option<String>,
    /// New last name. It cannot be cleared.
    #[serde(default)]
    pub last_name: Option<String>,
    /// New company name. An empty string clears it.
    #[serde(default)]
    pub company: Option<String>,
    /// New email address. An empty string clears it.
    #[serde(default)]
    pub email: Option<String>,
    /// New mobile number in E.164 form, e.g. `+14155552309`. An empty
    /// string clears it.
    #[serde(default)]
    pub mobile: Option<String>,
    /// Consent to receive SMS. `false` clears it.
    #[serde(default)]
    pub sms_consent: Option<bool>,
    /// Reuse the billing address as the shipping address. `false` stops that.
    #[serde(default)]
    pub use_billing_as_shipping: Option<bool>,
    /// New AVS billing street line 1. An empty string clears it.
    #[serde(default)]
    pub billing_line1: Option<String>,
    /// New AVS billing street line 2. An empty string clears it.
    #[serde(default)]
    pub billing_line2: Option<String>,
    /// New AVS billing city. An empty string clears it.
    #[serde(default)]
    pub billing_city: Option<String>,
    /// New AVS billing state code, e.g. `CO`. The ISO 3166-2 subdivision
    /// code, which for a US state is the two-letter USPS abbreviation. An
    /// empty string clears it.
    #[serde(default)]
    pub billing_state: Option<String>,
    /// New AVS billing postal code. An empty string clears it.
    #[serde(default)]
    pub billing_postal_code: Option<String>,
    /// New AVS billing country code, e.g. `US`. ISO 3166-1 alpha-2. An empty
    /// string clears it.
    #[serde(default)]
    pub billing_country: Option<String>,
    /// New shipping street line 1. An empty string clears it.
    #[serde(default)]
    pub shipping_line1: Option<String>,
    /// New shipping street line 2. An empty string clears it.
    #[serde(default)]
    pub shipping_line2: Option<String>,
    /// New shipping city. An empty string clears it.
    #[serde(default)]
    pub shipping_city: Option<String>,
    /// New shipping state code, e.g. `CO`. The ISO 3166-2 subdivision code,
    /// which for a US state is the two-letter USPS abbreviation. An empty
    /// string clears it.
    #[serde(default)]
    pub shipping_state: Option<String>,
    /// New shipping postal code. An empty string clears it.
    #[serde(default)]
    pub shipping_postal_code: Option<String>,
    /// New shipping country code, e.g. `US`. ISO 3166-1 alpha-2. An empty
    /// string clears it.
    #[serde(default)]
    pub shipping_country: Option<String>,
}

#[tool_router(router = customers_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "Get a customer by id. Safe to retry.")]
    pub async fn customers_get(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["customers".into(), "get".into(), p.id]);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "List customers. `page_index` is 0-based and `page_size` is 1–100; `all` walks every page and cannot be combined with `page_index`. Safe to retry."
    )]
    pub async fn customers_list(
        &self,
        Parameters(p): Parameters<CustomersList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["customers".into(), "list".into()]);
        PageArgs {
            page_index: p.page_index,
            page_size: p.page_size,
            all: p.all,
            sort_by: p.sort_by,
            asc: p.asc,
            desc: p.desc,
        }
        .push(&mut args);
        opt(&mut args, "--full-name", p.full_name);
        opt(&mut args, "--email", p.email);
        opt(&mut args, "--company-name", p.company_name);
        opt(&mut args, "--mobile", p.mobile);
        opt(&mut args, "--created-from", p.created_from);
        opt(&mut args, "--created-to", p.created_to);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Create a customer. NOT idempotent and it takes no reference id, so a repeat creates a second record — reconcile with customers_list before re-issuing after an ambiguous failure."
    )]
    pub async fn customers_create(
        &self,
        Parameters(p): Parameters<CustomerCreate>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_create") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "customers".into(),
            "create".into(),
            "--first-name".into(),
            p.first_name,
            "--last-name".into(),
            p.last_name,
        ]);
        opt(&mut args, "--company", p.company);
        opt(&mut args, "--email", p.email);
        opt(&mut args, "--mobile", p.mobile);
        switch(&mut args, "--sms-consent", p.sms_consent);
        switch(
            &mut args,
            "--use-billing-as-shipping",
            p.use_billing_as_shipping,
        );
        AddressArgs {
            line1: p.billing_line1,
            line2: p.billing_line2,
            city: p.billing_city,
            state: p.billing_state,
            postal_code: p.billing_postal_code,
            country: p.billing_country,
        }
        .push(&mut args, "billing");
        AddressArgs {
            line1: p.shipping_line1,
            line2: p.shipping_line2,
            city: p.shipping_city,
            state: p.shipping_state,
            postal_code: p.shipping_postal_code,
            country: p.shipping_country,
        }
        .push(&mut args, "shipping");
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Update a customer. Omitted fields keep their stored values. The success is a confirmation carrying `customerId` and `updated: true`, not the updated record — issue customers_get to read one back. Safe to retry."
    )]
    pub async fn customers_update(
        &self,
        Parameters(p): Parameters<CustomerUpdate>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_update") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend(["customers".into(), "update".into(), p.id]);
        let no_fields = args.len();
        opt(&mut args, "--first-name", p.first_name);
        opt(&mut args, "--last-name", p.last_name);
        opt(&mut args, "--company", p.company);
        opt(&mut args, "--email", p.email);
        opt(&mut args, "--mobile", p.mobile);
        toggle(&mut args, "--sms-consent", p.sms_consent);
        toggle(
            &mut args,
            "--use-billing-as-shipping",
            p.use_billing_as_shipping,
        );
        AddressArgs {
            line1: p.billing_line1,
            line2: p.billing_line2,
            city: p.billing_city,
            state: p.billing_state,
            postal_code: p.billing_postal_code,
            country: p.billing_country,
        }
        .push(&mut args, "billing");
        AddressArgs {
            line1: p.shipping_line1,
            line2: p.shipping_line2,
            city: p.shipping_city,
            state: p.shipping_state,
            postal_code: p.shipping_postal_code,
            country: p.shipping_country,
        }
        .push(&mut args, "shipping");
        // The CLI refuses an update that names no field before the wire, so
        // refusing it here saves the spawn and answers with the same `client` kind.
        if args.len() == no_fields {
            return Ok(flute_err_to_result(FluteError::Client {
                message: "nothing to update: pass at least one field besides `id`".into(),
            }));
        }
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Delete a customer. A 404 is a success, so re-running is a safe no-op, but its confirmation carries `deleted: false, found: false`: the server answers alike for an id already deleted and one that never existed. `deleted: true` means this call removed the customer. Deleting a customer also removes their saved payment methods: payment_methods_get on one then answers 404."
    )]
    pub async fn customers_delete(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_delete") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend(["customers".into(), "delete".into(), p.id, "--yes".into()]);
        Ok(self.run_tool(args).await)
    }
}

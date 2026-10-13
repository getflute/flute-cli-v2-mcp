use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::address::AddressArgs;
use crate::tools::{Id, KeyedId, PageArgs, opt, repeat, switch};

/// The instrument and contact flags `transactions create` and
/// `transactions credit` both accept. Shared as argv construction only; each
/// params struct still declares every field, so `deny_unknown_fields` stays in
/// force.
#[derive(Default)]
struct ChargeArgs {
    currency_code: Option<String>,
    customer_id: Option<String>,
    card: Option<String>,
    exp: Option<String>,
    cvv: Option<String>,
    payment_method_id: Option<String>,
    instrument: Option<String>,
    ach_account_number: Option<String>,
    ach_routing_number: Option<String>,
    ach_account_type: Option<String>,
    ach_account_holder_type: Option<String>,
    ach_tax_id: Option<String>,
    sec_code: Option<String>,
    requester_ip: Option<String>,
    same_day: Option<bool>,
    contact_first_name: Option<String>,
    contact_last_name: Option<String>,
    contact_company: Option<String>,
    contact_email: Option<String>,
    contact_phone: Option<String>,
    contact_sms_consent: Option<bool>,
}

impl ChargeArgs {
    fn push(self, args: &mut Vec<String>) {
        opt(args, "--currency-code", self.currency_code);
        opt(args, "--customer-id", self.customer_id);
        opt(args, "--card", self.card);
        opt(args, "--exp", self.exp);
        opt(args, "--cvv", self.cvv);
        opt(args, "--payment-method-id", self.payment_method_id);
        opt(args, "--instrument", self.instrument);
        opt(args, "--ach-account-number", self.ach_account_number);
        opt(args, "--ach-routing-number", self.ach_routing_number);
        opt(args, "--ach-account-type", self.ach_account_type);
        opt(
            args,
            "--ach-account-holder-type",
            self.ach_account_holder_type,
        );
        opt(args, "--ach-tax-id", self.ach_tax_id);
        opt(args, "--sec-code", self.sec_code);
        opt(args, "--requester-ip", self.requester_ip);
        switch(args, "--same-day", self.same_day);
        opt(args, "--contact-first-name", self.contact_first_name);
        opt(args, "--contact-last-name", self.contact_last_name);
        opt(args, "--contact-company", self.contact_company);
        opt(args, "--contact-email", self.contact_email);
        opt(args, "--contact-phone", self.contact_phone);
        switch(args, "--contact-sms-consent", self.contact_sms_consent);
    }
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransactionsList {
    /// Zero-based page index.
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub page_index: Option<u32>,
    /// Items per page, 1–100.
    #[schemars(range(min = 1, max = 100))]
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub page_size: Option<u32>,
    /// Walk every page. Cannot be combined with `page_index`, and the response
    /// it returns carries no `meta.page_info`, so the per-page counts are absent.
    /// Every matching row comes back in one result, which on a busy account
    /// runs to hundreds of rows; narrow it with a date, status or
    /// `reference_id` filter, or page with `page_size`.
    #[serde(default)]
    pub all: Option<bool>,
    /// One of `transactionDateTime`, `processedAmount`, `transactionStatus`,
    /// `maskedCardNumber`, `cardTokenType`, `transactionId`, `paymentMethodType`,
    /// `customerName`, `customerCompanyName` or `merchantCompanyName`. Any
    /// other spelling, a different case included, is refused by the CLI with
    /// `kind: "client"`. With neither `sort_by`, `asc` nor `desc`, results
    /// come back newest first; with `sort_by` alone, descending by that
    /// field.
    #[serde(default)]
    pub sort_by: Option<String>,
    /// Sort ascending, by `sort_by` or else by transaction date. Cannot be
    /// combined with `desc`.
    #[serde(default)]
    pub asc: Option<bool>,
    /// Sort descending, by `sort_by` or else by transaction date. Cannot be
    /// combined with `asc`.
    #[serde(default)]
    pub desc: Option<bool>,
    /// Filter to transactions created on or after this date, inclusive. ISO
    /// 8601, whole-day granularity. A time does not move the filter, but the
    /// API still refuses a `from` later than `to`, times included.
    #[serde(default)]
    pub from: Option<String>,
    /// Filter to transactions created on or before this date, inclusive. ISO
    /// 8601, whole-day granularity; it must be before `9999-12-31`, which the
    /// API refuses.
    #[serde(default)]
    pub to: Option<String>,
    /// `api-key`, `invoice`, `mobile-app`, `portal`, `quick-payment`, `subscription`, `tap-to-pay`, `terminal` or `web-component`.
    #[serde(default)]
    pub source_type: Option<String>,
    /// Filter by the source identifier — the api key, invoice or subscription
    /// id that created the transaction.
    #[serde(default)]
    pub source_id: Option<String>,
    /// Filter by settlement batch id. Three states: omit it for no batch
    /// filter, pass a UUID for that batch, or pass the literal string `null`
    /// for transactions not yet settled into one.
    #[serde(default)]
    pub batch_id: Option<String>,
    /// One of `authorized`, `cancelled`, `captured`, `charged-back`, `cleared`, `declined`, `failed`, `held`, `held-by-processor`, `informational`, `in-progress`, `partially-authorized`, `pending`, `refunded`, `scheduled`, `settled`, `verified`, `voided`. An equality filter; there is no "not settled". The response reports `transactionStatus` in a different vocabulary, so a value read back is not necessarily one you can filter by, and a mismatch answers an empty page rather than an error.
    #[serde(default)]
    pub status: Option<String>,
    /// `Card`, `ACH` or `Cash`.
    #[serde(default)]
    pub payment_method_type: Option<String>,
    /// Filter by customer id.
    #[serde(default)]
    pub customer_id: Option<String>,
    #[serde(default)]
    pub merchant_id: Option<String>,
    /// Plain decimal, e.g. `10.00`.
    #[serde(default)]
    pub min_amount: Option<String>,
    /// Plain decimal, e.g. `500.00`. An inclusive upper bound; it cannot be
    /// negative and must be at least `min_amount`.
    #[serde(default)]
    pub max_amount: Option<String>,
    /// Filter by the merchant-assigned reference. An exact match.
    #[serde(default)]
    pub reference_id: Option<String>,
}

#[derive(Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransactionCreate {
    /// Plain decimal, e.g. `100.00`.
    pub amount: String,
    /// Processor UUID. `settings_payment_config` lists the account's, one per
    /// payment method.
    pub payment_processor_id: String,
    /// `auto` charges immediately, `manual` authorises for a later capture.
    #[serde(default)]
    pub capture_method: Option<String>,
    /// Part of the duplicate-check key on card authorisations, alongside the
    /// merchant, the amount, the transaction type and the card's last four,
    /// within a sliding window. Repeating one refuses the charge with a 400
    /// naming `IsDuplicate`; vary it to charge the same card and amount again.
    /// Two cautions. The API's message for that refusal names the amount, the
    /// customer details and the timestamp but not this field. And the check is
    /// a per-merchant setting rather than a guarantee of the endpoint, so a
    /// merchant with it switched off accepts the repeat and charges twice.
    #[serde(default)]
    pub reference_id: Option<String>,
    /// ISO 4217 currency code. Only `USD` is supported, and it defaults to
    /// `USD`.
    #[serde(default)]
    pub currency_code: Option<String>,
    /// An existing customer to link this to. The id is echoed back on the
    /// response.
    #[serde(default)]
    pub customer_id: Option<String>,
    /// Card PAN, 13 to 19 digits, e.g. `4111111111111111`. Send it with `exp`
    /// and `cvv`; the three go together or none do.
    #[serde(default)]
    pub card: Option<String>,
    /// `MM/YY` or `MM/YYYY`.
    #[serde(default)]
    pub exp: Option<String>,
    /// Card security code, three or four digits.
    #[serde(default)]
    pub cvv: Option<String>,
    /// A stored card or bank account to charge instead of raw details. Pair
    /// it with `instrument`, because a stored id does not say which container
    /// it belongs in.
    #[serde(default)]
    pub payment_method_id: Option<String>,
    /// Which container `payment_method_id` belongs in: `card` or `ach`.
    #[serde(default)]
    pub instrument: Option<String>,
    /// Bank account number, e.g. `1234567890`. Send it with
    /// `ach_routing_number`, `ach_account_type` and
    /// `ach_account_holder_type`; the four go together or none do.
    #[serde(default)]
    pub ach_account_number: Option<String>,
    /// ABA routing number, e.g. `021000021`. Send it with
    /// `ach_account_number`, `ach_account_type` and
    /// `ach_account_holder_type`.
    #[serde(default)]
    pub ach_routing_number: Option<String>,
    /// `checking` or `savings`.
    #[serde(default)]
    pub ach_account_type: Option<String>,
    /// `business` or `personal`.
    #[serde(default)]
    pub ach_account_holder_type: Option<String>,
    /// Tax ID of the account holder, e.g. `12-3456789`.
    #[serde(default)]
    pub ach_tax_id: Option<String>,
    /// `web`, `ppd` or `ccd`. Required for ACH.
    #[serde(default)]
    pub sec_code: Option<String>,
    /// IPv4 or IPv6 address of the end user submitting the transaction, at
    /// most 45 characters. Required for ACH; the CLI sends `127.0.0.1` when
    /// it is omitted.
    #[serde(default)]
    pub requester_ip: Option<String>,
    /// Request same-day ACH settlement. Only `true` has an effect here.
    #[serde(default)]
    pub same_day: Option<bool>,
    /// Contact first name. A new ACH instrument requires it together with
    /// `contact_last_name`, or `contact_company` in their place.
    #[serde(default)]
    pub contact_first_name: Option<String>,
    /// Contact last name. A new ACH instrument requires it together with
    /// `contact_first_name`, or `contact_company` in their place.
    #[serde(default)]
    pub contact_last_name: Option<String>,
    /// Contact company name. It satisfies the new-ACH name requirement in
    /// place of the first and last pair, and a business ACH account needs
    /// it.
    #[serde(default)]
    pub contact_company: Option<String>,
    /// Contact email address. Required for a new ACH instrument.
    #[serde(default)]
    pub contact_email: Option<String>,
    /// Contact mobile number in E.164 form, e.g. `+14155552309`. Required
    /// for a new ACH instrument.
    #[serde(default)]
    pub contact_phone: Option<String>,
    /// Record the contact's consent to receive SMS. Only `true` has an
    /// effect here.
    #[serde(default)]
    pub contact_sms_consent: Option<bool>,
    /// Tip as a plain decimal, e.g. `5.00`. At least 0.01, and mutually
    /// exclusive with `tip_rate`. Refused with a 400 on an account with tips
    /// disabled; settings_payment_config shows the setting as `isTipsEnabled`.
    #[serde(default)]
    pub tip_amount: Option<String>,
    /// Percentage, e.g. `18.5`.
    #[serde(default)]
    pub tip_rate: Option<String>,
    /// Discount as a plain decimal, e.g. `5.00`. At least 0.01, and mutually
    /// exclusive with `discount_rate`.
    #[serde(default)]
    pub discount_amount: Option<String>,
    /// Discount as a percentage of the subtotal, e.g. `10` for 10%. Mutually
    /// exclusive with `discount_amount`. Refused with a 400 ("Percentage off
    /// is disabled") on an account without percentage-off, a setting
    /// settings_payment_config does not show.
    #[serde(default)]
    pub discount_rate: Option<String>,
    /// Card surcharge as a percentage of the base amount, at most 3, e.g.
    /// `2.5` for 2.5%, applied after any discount. Applied only when
    /// settings_payment_config shows `zeroCostProcessingOption: "Surcharge"`;
    /// otherwise it is dropped without an error and the response reads
    /// `surchargeAmount: 0`.
    #[serde(default)]
    pub surcharge_rate: Option<String>,
    /// Level-2 sales tax rate as a percentage, 0.01 to 100, e.g. `8.5`. The
    /// processor field is four characters wide. Like every level-2 and
    /// level-3 field here, transactions_get does not report it back.
    #[serde(default)]
    pub l2_tax_rate: Option<String>,
    /// Level-3 VAT invoice number, 1 to 15 characters; letters, numbers and
    /// spaces only.
    #[serde(default)]
    pub l3_invoice: Option<String>,
    /// Level-3 purchase order number, the value the buyer issues to the
    /// seller to identify the order.
    #[serde(default)]
    pub l3_po: Option<String>,
    /// Level-3 duty charges rate as a percentage, 0 to 100, e.g. `2.5`. The
    /// transaction read does not report it back.
    #[serde(default)]
    pub l3_duty_rate: Option<String>,
    /// Level-3 shipping or freight charge as a plain decimal, e.g. `9.99`.
    /// Zero or greater.
    #[serde(default)]
    pub l3_shipping: Option<String>,
    /// One line item per entry, as comma-separated `key=value` pairs. Keys:
    /// `productName`, `productDescription`, `productCode`, `unitPrice`,
    /// `measurementUnit`, `quantity`, `taxAmount`, `discountRate`.
    #[serde(default)]
    pub l3_product: Vec<String>,
    /// Mark the transaction as customer-initiated (CIT) rather than
    /// merchant-initiated (MIT). Only `true` has an effect here.
    #[serde(default)]
    pub customer_initiated: Option<bool>,
    /// `card` or `cash`.
    #[serde(default)]
    pub pricing_type: Option<String>,
    /// AVS billing street line 1. At most 70 characters on an ACH
    /// transaction.
    #[serde(default)]
    pub billing_line1: Option<String>,
    /// AVS billing street line 2.
    #[serde(default)]
    pub billing_line2: Option<String>,
    /// AVS billing city. Required for ACH, and at most 70 characters.
    #[serde(default)]
    pub billing_city: Option<String>,
    /// AVS billing state code, e.g. `CO`. An ISO 3166-2 subdivision of
    /// `billing_country`; required for a US ACH address.
    #[serde(default)]
    pub billing_state: Option<String>,
    /// AVS billing postal code. 5 to 12 characters for a card; for ACH, a US
    /// ZIP in `#####` or `#####-####` form.
    #[serde(default)]
    pub billing_postal_code: Option<String>,
    /// AVS billing country code, e.g. `US`. ISO 3166-1 alpha-2; for ACH it
    /// must be `US`, `CA` or `PR`.
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
    /// Shipping state code, e.g. `CO`. An ISO 3166-2 subdivision of
    /// `shipping_country`.
    #[serde(default)]
    pub shipping_state: Option<String>,
    /// Shipping postal code.
    #[serde(default)]
    pub shipping_postal_code: Option<String>,
    /// Shipping country code, e.g. `US`. ISO 3166-1 alpha-2.
    #[serde(default)]
    pub shipping_country: Option<String>,
    /// Sent as `Idempotency-Key`. Pass a fresh one, e.g. a UUID, on the
    /// first attempt; re-sending the same key with the same arguments returns
    /// the first result instead of repeating the operation. The same key with
    /// different arguments answers 422, and a key is not scoped to one tool,
    /// so use a new key for each distinct operation. At most 255 printable
    /// ASCII characters.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransactionCredit {
    /// Plain decimal, e.g. `500.00`.
    pub amount: String,
    /// Processor UUID. `settings_payment_config` lists the account's, one per
    /// payment method.
    pub payment_processor_id: String,
    /// Required here. It does not dedupe: two credits carrying the same value
    /// both settle and move money twice. It is your handle for reconciling
    /// with transactions_list, which does filter on it — until an ACH hold or
    /// release replaces it, after which that reconcile answers `totalItems: 0`
    /// for a credit that is still live.
    pub reference_id: String,
    /// ISO 4217 currency code. Only `USD` is supported, and it defaults to
    /// `USD`.
    #[serde(default)]
    pub currency_code: Option<String>,
    /// An existing customer to link this credit to. The id is echoed back on
    /// the response.
    #[serde(default)]
    pub customer_id: Option<String>,
    /// Card PAN, 13 to 19 digits, e.g. `4111111111111111`. Send it with `exp`
    /// and `cvv`; the three go together or none do.
    #[serde(default)]
    pub card: Option<String>,
    /// Card expiry in `MM/YY` or `MM/YYYY` form, e.g. `12/26`.
    #[serde(default)]
    pub exp: Option<String>,
    /// Card security code, three or four digits.
    #[serde(default)]
    pub cvv: Option<String>,
    /// A stored ACH method. The API's credit request carries no field for a
    /// stored card, so pairing this with `instrument: "card"` is refused with
    /// `kind: "client"` before any request; a card credit takes `card`, `exp`
    /// and `cvv`.
    #[serde(default)]
    pub payment_method_id: Option<String>,
    /// Which container `payment_method_id` belongs in: `card` or `ach`. A
    /// credit carries no field for a stored card, so `card` here answers a
    /// 400.
    #[serde(default)]
    pub instrument: Option<String>,
    /// Bank account number, e.g. `1234567890`. Send it with
    /// `ach_routing_number`, `ach_account_type` and
    /// `ach_account_holder_type`; the four go together or none do.
    #[serde(default)]
    pub ach_account_number: Option<String>,
    /// ABA routing number, e.g. `021000021`. Send it with
    /// `ach_account_number`, `ach_account_type` and
    /// `ach_account_holder_type`.
    #[serde(default)]
    pub ach_routing_number: Option<String>,
    /// `checking` or `savings`.
    #[serde(default)]
    pub ach_account_type: Option<String>,
    /// `business` or `personal`.
    #[serde(default)]
    pub ach_account_holder_type: Option<String>,
    /// Tax ID of the account holder, e.g. `12-3456789`.
    #[serde(default)]
    pub ach_tax_id: Option<String>,
    /// ACH SEC code under the NACHA standard: `web`, `ppd` or `ccd`. Required
    /// for ACH.
    #[serde(default)]
    pub sec_code: Option<String>,
    /// IPv4 or IPv6 address of the end user submitting the transaction, at
    /// most 45 characters. Required for ACH; the CLI sends `127.0.0.1` when
    /// it is omitted.
    #[serde(default)]
    pub requester_ip: Option<String>,
    /// Request same-day ACH settlement. Only `true` has an effect here.
    #[serde(default)]
    pub same_day: Option<bool>,
    /// Contact first name. A new ACH instrument requires it together with
    /// `contact_last_name`, or `contact_company` in their place.
    #[serde(default)]
    pub contact_first_name: Option<String>,
    /// Contact last name. A new ACH instrument requires it together with
    /// `contact_first_name`, or `contact_company` in their place.
    #[serde(default)]
    pub contact_last_name: Option<String>,
    /// Contact company name. It satisfies the new-ACH name requirement in
    /// place of the first and last pair, and a business ACH account needs
    /// it.
    #[serde(default)]
    pub contact_company: Option<String>,
    /// Contact email address. Required for a new ACH instrument.
    #[serde(default)]
    pub contact_email: Option<String>,
    /// Contact mobile number in E.164 form, e.g. `+14155552309`. Required
    /// for a new ACH instrument.
    #[serde(default)]
    pub contact_phone: Option<String>,
    /// Record the contact's consent to receive SMS. Only `true` has an
    /// effect here.
    #[serde(default)]
    pub contact_sms_consent: Option<bool>,
    /// AVS billing street line 1. At most 70 characters on an ACH
    /// transaction.
    #[serde(default)]
    pub billing_line1: Option<String>,
    /// AVS billing street line 2.
    #[serde(default)]
    pub billing_line2: Option<String>,
    /// AVS billing city. Required for ACH, and at most 70 characters.
    #[serde(default)]
    pub billing_city: Option<String>,
    /// AVS billing state code, e.g. `CO`. An ISO 3166-2 subdivision of
    /// `billing_country`; required for a US ACH address.
    #[serde(default)]
    pub billing_state: Option<String>,
    /// AVS billing postal code. 5 to 12 characters for a card; for ACH, a US
    /// ZIP in `#####` or `#####-####` form.
    #[serde(default)]
    pub billing_postal_code: Option<String>,
    /// AVS billing country code, e.g. `US`. ISO 3166-1 alpha-2; for ACH it
    /// must be `US`, `CA` or `PR`.
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
    /// Shipping state code, e.g. `CO`. An ISO 3166-2 subdivision of
    /// `shipping_country`.
    #[serde(default)]
    pub shipping_state: Option<String>,
    /// Shipping postal code.
    #[serde(default)]
    pub shipping_postal_code: Option<String>,
    /// Shipping country code, e.g. `US`. ISO 3166-1 alpha-2.
    #[serde(default)]
    pub shipping_country: Option<String>,
    /// Sent as `Idempotency-Key`. Pass a fresh one, e.g. a UUID, on the
    /// first attempt; re-sending the same key with the same arguments returns
    /// the first result instead of repeating the operation. The same key with
    /// different arguments answers 422, and a key is not scoped to one tool,
    /// so use a new key for each distinct operation. At most 255 printable
    /// ASCII characters.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// `capture` and `reversal`: the transaction id as a flag, plus an optional
/// partial amount.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransactionAmount {
    /// The transaction to act on, as a UUID: capture takes a previously
    /// authorised one, reversal takes one to void or refund.
    pub transaction_id: String,
    /// Omit for the full amount.
    #[serde(default)]
    pub amount: Option<String>,
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
pub struct TipAdjust {
    /// The card transaction to adjust, as a UUID.
    pub transaction_id: String,
    /// New tip as a plain decimal, e.g. `3.50`. Mutually exclusive with
    /// `tip_rate`.
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
pub struct ShareReceipt {
    /// Transaction id.
    pub id: String,
    /// `sms`, the only channel this endpoint has. The CLI refuses any other
    /// value before the round trip, so a wrong one answers `kind: "client"`.
    pub share_by: String,
    /// Mobile number in E.164 form.
    pub recipient: String,
    /// The customer has consented. Required.
    #[serde(default)]
    pub consent: Option<bool>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CalculateAmount {
    /// Plain decimal, e.g. `100.00`.
    pub amount: String,
    /// ISO 4217 currency code. Only `USD` is supported, and it defaults to
    /// `USD`.
    #[serde(default)]
    pub currency_code: Option<String>,
    /// `card` or `cash`.
    #[serde(default)]
    pub pricing_type: Option<String>,
    /// Tip as a plain decimal, e.g. `5.00`.
    #[serde(default)]
    pub tip_amount: Option<String>,
    /// Tip as a percentage, e.g. `15` for 15%.
    #[serde(default)]
    pub tip_rate: Option<String>,
    /// Discount as a plain decimal, e.g. `5.00`. Mutually exclusive with
    /// `discount_rate`.
    #[serde(default)]
    pub discount_amount: Option<String>,
    /// Discount as a percentage, e.g. `10` for 10%. On an account without
    /// percentage-off it is ignored and the result reads `discountAmount: 0`.
    #[serde(default)]
    pub discount_rate: Option<String>,
    /// Card surcharge as a percentage, at most 3, e.g. `2.5` for 2.5%. Omit
    /// it to use the merchant account's own rate. Applied only when
    /// settings_payment_config shows `zeroCostProcessingOption: "Surcharge"`;
    /// otherwise the result reads `surchargeAmount: 0`.
    #[serde(default)]
    pub surcharge_rate: Option<String>,
}

#[tool_router(router = transactions_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(
        description = "Get a transaction by id. Safe to retry. This is the read that reports a transaction's current state; a replayed write does not."
    )]
    pub async fn transactions_get(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["transactions".into(), "get".into(), p.id]);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "List transactions. `page_index` is 0-based and `page_size` is 1–100; `all` walks every page and cannot be combined with `page_index`. Safe to retry."
    )]
    pub async fn transactions_list(
        &self,
        Parameters(p): Parameters<TransactionsList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["transactions".into(), "list".into()]);
        PageArgs {
            page_index: p.page_index,
            page_size: p.page_size,
            all: p.all,
            sort_by: p.sort_by,
            asc: p.asc,
            desc: p.desc,
        }
        .push(&mut args);
        opt(&mut args, "--from", p.from);
        opt(&mut args, "--to", p.to);
        opt(&mut args, "--source-type", p.source_type);
        opt(&mut args, "--source-id", p.source_id);
        opt(&mut args, "--batch-id", p.batch_id);
        opt(&mut args, "--status", p.status);
        opt(&mut args, "--payment-method-type", p.payment_method_type);
        opt(&mut args, "--customer-id", p.customer_id);
        opt(&mut args, "--merchant-id", p.merchant_id);
        opt(&mut args, "--min-amount", p.min_amount);
        opt(&mut args, "--max-amount", p.max_amount);
        opt(&mut args, "--reference-id", p.reference_id);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Calculate the totals for an amount without creating anything. It applies the merchant's settings: a discount or surcharge the account does not allow comes back as 0, not as an error, so a zero is not proof the inputs were wrong. A POST that changes no state, so it is a read and runs on production. Safe to retry."
    )]
    pub async fn transactions_calculate_amount(
        &self,
        Parameters(p): Parameters<CalculateAmount>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend([
            "transactions".into(),
            "calculate-amount".into(),
            "--amount".into(),
            p.amount,
        ]);
        opt(&mut args, "--currency-code", p.currency_code);
        opt(&mut args, "--pricing-type", p.pricing_type);
        opt(&mut args, "--tip-amount", p.tip_amount);
        opt(&mut args, "--tip-rate", p.tip_rate);
        opt(&mut args, "--discount-amount", p.discount_amount);
        opt(&mut args, "--discount-rate", p.discount_rate);
        opt(&mut args, "--surcharge-rate", p.surcharge_rate);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Charge a card or bank account. A DECLINE IS A SUCCESS: the API answers it with HTTP 200, so read `data.transactionStatus` to learn whether money moved. NOT safe to retry without `idempotency_key`: pass a fresh one, and after an ambiguous failure re-send the SAME key with the same arguments to get the first result back instead of a repeat. A replay returns the first response as it was, an error included, not the current state. Requests sent at the same moment with one key answer 409 `IdempotencyKeyInProgress` for all but the first: wait, then re-send the same key — a new key charges again. Re-send the SAME `reference_id` as well: on a card sale it is part of a duplicate check that refuses a repeat sent after the first, and varying it escapes that check. Card sales sent at the same moment with one reference all went through, and an ACH sale has no such check at all. Without a key, reconcile with transactions_list before re-issuing. Use `capture_method: \"manual\"` to authorise without capturing."
    )]
    pub async fn transactions_create(
        &self,
        Parameters(p): Parameters<TransactionCreate>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_create") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "transactions".into(),
            "create".into(),
            "--amount".into(),
            p.amount,
            "--payment-processor-id".into(),
            p.payment_processor_id,
        ]);
        opt(&mut args, "--capture-method", p.capture_method);
        opt(&mut args, "--reference-id", p.reference_id);
        ChargeArgs {
            currency_code: p.currency_code,
            customer_id: p.customer_id,
            card: p.card,
            exp: p.exp,
            cvv: p.cvv,
            payment_method_id: p.payment_method_id,
            instrument: p.instrument,
            ach_account_number: p.ach_account_number,
            ach_routing_number: p.ach_routing_number,
            ach_account_type: p.ach_account_type,
            ach_account_holder_type: p.ach_account_holder_type,
            ach_tax_id: p.ach_tax_id,
            sec_code: p.sec_code,
            requester_ip: p.requester_ip,
            same_day: p.same_day,
            contact_first_name: p.contact_first_name,
            contact_last_name: p.contact_last_name,
            contact_company: p.contact_company,
            contact_email: p.contact_email,
            contact_phone: p.contact_phone,
            contact_sms_consent: p.contact_sms_consent,
        }
        .push(&mut args);
        opt(&mut args, "--tip-amount", p.tip_amount);
        opt(&mut args, "--tip-rate", p.tip_rate);
        opt(&mut args, "--discount-amount", p.discount_amount);
        opt(&mut args, "--discount-rate", p.discount_rate);
        opt(&mut args, "--surcharge-rate", p.surcharge_rate);
        opt(&mut args, "--l2-tax-rate", p.l2_tax_rate);
        opt(&mut args, "--l3-invoice", p.l3_invoice);
        opt(&mut args, "--l3-po", p.l3_po);
        opt(&mut args, "--l3-duty-rate", p.l3_duty_rate);
        opt(&mut args, "--l3-shipping", p.l3_shipping);
        repeat(&mut args, "--l3-product", p.l3_product);
        switch(&mut args, "--customer-initiated", p.customer_initiated);
        opt(&mut args, "--pricing-type", p.pricing_type);
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
        opt(&mut args, "--idempotency-key", p.idempotency_key);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Card or ACH credit payment, which moves money OUT. A DECLINE IS A SUCCESS — read `data.transactionStatus`. NOT safe to retry without `idempotency_key`: pass a fresh one, and after an ambiguous failure re-send the SAME key with the same arguments to get the first result back instead of a repeat. A replay returns the first response as it was, an error included, not the current state. `reference_id` is required but does not dedupe, so re-issuing without the key credits a second time rather than being refused. Without a key, reconcile with transactions_list on that reference before every re-issue, unless an ACH hold, release or reversal has replaced it — then reconcile by transaction id with transactions_get."
    )]
    pub async fn transactions_credit(
        &self,
        Parameters(p): Parameters<TransactionCredit>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_credit") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "transactions".into(),
            "credit".into(),
            "--amount".into(),
            p.amount,
            "--payment-processor-id".into(),
            p.payment_processor_id,
            "--reference-id".into(),
            p.reference_id,
        ]);
        ChargeArgs {
            currency_code: p.currency_code,
            customer_id: p.customer_id,
            card: p.card,
            exp: p.exp,
            cvv: p.cvv,
            payment_method_id: p.payment_method_id,
            instrument: p.instrument,
            ach_account_number: p.ach_account_number,
            ach_routing_number: p.ach_routing_number,
            ach_account_type: p.ach_account_type,
            ach_account_holder_type: p.ach_account_holder_type,
            ach_tax_id: p.ach_tax_id,
            sec_code: p.sec_code,
            requester_ip: p.requester_ip,
            same_day: p.same_day,
            contact_first_name: p.contact_first_name,
            contact_last_name: p.contact_last_name,
            contact_company: p.contact_company,
            contact_email: p.contact_email,
            contact_phone: p.contact_phone,
            contact_sms_consent: p.contact_sms_consent,
        }
        .push(&mut args);
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
        opt(&mut args, "--idempotency-key", p.idempotency_key);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Capture a previously authorised transaction. Omit `amount` for a full capture. NOT safe to retry without `idempotency_key`: pass a fresh one, and after an ambiguous failure re-send the SAME key with the same arguments to get the first result back instead of a repeat. A replay returns the first response as it was, an error included, not the current state. A 409 means the API refused this request for the transaction's current state — already captured or voided, declined, an ACH sale in progress, or the same key still in flight — and is not proof that anything happened: read transactions_get, and do not follow its advice to use a different idempotency key. Without a key, reconcile with transactions_get before re-issuing. A 404 here is an error, not a success."
    )]
    pub async fn transactions_capture(
        &self,
        Parameters(p): Parameters<TransactionAmount>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_capture") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "transactions".into(),
            "capture".into(),
            "--transaction-id".into(),
            p.transaction_id,
        ]);
        opt(&mut args, "--amount", p.amount);
        opt(&mut args, "--idempotency-key", p.idempotency_key);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Void or refund a transaction — one endpoint for both, and whether it has settled is detected server-side. Omit `amount` for a full reversal; a partial `amount` is refused on a card sale that has not settled and on an ACH sale. An ACH sale can be voided only while `Scheduled`, which lasts seconds; once `InProgress`, every reversal answers 409 until it reaches `Cleared`, about a quarter hour after creation, and from then on it is refunded. NOT safe to retry without `idempotency_key`: pass a fresh one, and after an ambiguous failure re-send the SAME key with the same arguments to get the first result back instead of a repeat. A replay returns the first response as it was, an error included, not the current state. A 409 means the API refused this request for the transaction's current state — already captured or voided, declined, an ACH sale in progress, or the same key still in flight — and is not proof that anything happened: read transactions_get, and do not follow its advice to use a different idempotency key. A repeat partial refund with a new key refunds again. Without a key, reconcile with transactions_get before re-issuing. The response is the original transaction, not the refund: the refund's id is the `originalTransactionId` of the refund event in its `transactionEvents`, and the refund is its own row in transactions_list. A void does not update the original's `refundDetails` or `availableRefundAmount`, and a `Cleared` ACH sale can report `availableRefundAmount: 0` and still refund in full. Voiding an ACH sale or reversing an ACH credit replaces its `reference_id`, so reconcile those by transaction id. The id of a refund voids that refund. A void creates no row of its own: it adds a `Void` event to the original's `transactionEvents`, and on an ACH sale that event's `amount` reads 0."
    )]
    pub async fn transactions_reversal(
        &self,
        Parameters(p): Parameters<TransactionAmount>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_reversal") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "transactions".into(),
            "reversal".into(),
            "--transaction-id".into(),
            p.transaction_id,
        ]);
        opt(&mut args, "--amount", p.amount);
        opt(&mut args, "--idempotency-key", p.idempotency_key);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Adjust the tip on a transaction. Allowed only when settings_payment_config lists `TipAdjustment` under `availableTransactionTypes`, which needs both tips and tip adjustment on for the account; otherwise every state answers 409 \"Transition is not allowed\". NOT safe to retry without `idempotency_key`: pass a fresh one, and after an ambiguous failure re-send the SAME key with the same arguments to get the first result back instead of a repeat. A replay returns the first response as it was, an error included, not the current state. A 409 means the API refused this request for the transaction's current state — already captured or voided, declined, an ACH sale in progress, or the same key still in flight — and is not proof that anything happened: read transactions_get, and do not follow its advice to use a different idempotency key. Without a key, reconcile with transactions_get before re-issuing."
    )]
    pub async fn transactions_tip_adjust(
        &self,
        Parameters(p): Parameters<TipAdjust>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_tip_adjust") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "transactions".into(),
            "tip-adjust".into(),
            "--transaction-id".into(),
            p.transaction_id,
        ]);
        opt(&mut args, "--tip-amount", p.tip_amount);
        opt(&mut args, "--tip-rate", p.tip_rate);
        opt(&mut args, "--idempotency-key", p.idempotency_key);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Place an ACH transaction on hold. NOT safe to retry without `idempotency_key`: pass a fresh one, and after an ambiguous failure re-send the SAME key with the same arguments to get the first result back instead of a repeat. A replay returns the first response as it was, an error included, not the current state. A 409 means the API refused this request for the transaction's current state — already captured or voided, declined, an ACH sale in progress, or the same key still in flight — and is not proof that anything happened: read transactions_get, and do not follow its advice to use a different idempotency key. It replaces the transaction's `reference_id` with a processor-assigned one, so the caller's own reference stops finding it through transactions_list — record the `transactionId` before calling and reconcile by it with transactions_get."
    )]
    pub async fn transactions_ach_hold(
        &self,
        Parameters(p): Parameters<KeyedId>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_ach_hold") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend(["transactions".into(), "ach-hold".into(), p.id]);
        opt(&mut args, "--idempotency-key", p.idempotency_key);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Release an ACH transaction from hold. NOT safe to retry without `idempotency_key`: pass a fresh one, and after an ambiguous failure re-send the SAME key with the same arguments to get the first result back instead of a repeat. A replay returns the first response as it was, an error included, not the current state. A 409 means the API refused this request for the transaction's current state — already captured or voided, declined, an ACH sale in progress, or the same key still in flight — and is not proof that anything happened: read transactions_get, and do not follow its advice to use a different idempotency key. It replaces the transaction's `reference_id` again, the way the hold did, so reconcile by `transactionId` with transactions_get."
    )]
    pub async fn transactions_ach_release(
        &self,
        Parameters(p): Parameters<KeyedId>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_ach_release") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend(["transactions".into(), "ach-release".into(), p.id]);
        opt(&mut args, "--idempotency-key", p.idempotency_key);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Send a transaction receipt to a customer. THIS SENDS A REAL MESSAGE. `share_by` takes `sms` and nothing else, and a wrong value is refused by the CLI rather than by the API, so it answers `kind: \"client\"` with no `status` and no `correlation_id`. `consent` is required. The success is a confirmation carrying `transactionId` and `shared: true`, which reports acceptance and not delivery."
    )]
    pub async fn transactions_share_receipt(
        &self,
        Parameters(p): Parameters<ShareReceipt>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_share_receipt") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "transactions".into(),
            "share-receipt".into(),
            p.id,
            "--share-by".into(),
            p.share_by,
            "--recipient".into(),
            p.recipient,
        ]);
        switch(&mut args, "--consent", p.consent);
        Ok(self.run_tool(args).await)
    }
}

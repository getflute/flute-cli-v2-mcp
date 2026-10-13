/// The six address fields the CLI accepts under both a `--billing-` and a
/// `--shipping-` prefix. Only the argv construction is shared: each tool's
/// params struct declares the twelve fields itself, because `#[serde(flatten)]`
/// would disable the `deny_unknown_fields` that turns a misspelled address
/// field into an error instead of a dropped one.
#[derive(Debug, Default)]
pub(crate) struct AddressArgs {
    pub line1: Option<String>,
    pub line2: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
}

impl AddressArgs {
    /// Append the set flags under `prefix` (`billing` or `shipping`), in the
    /// CLI's declared order. Nothing is appended when every field is `None`, so
    /// an untouched address stays absent from the request rather than blank.
    pub(crate) fn push(self, args: &mut Vec<String>, prefix: &str) {
        let mut put = |suffix: &str, value: Option<String>| {
            if let Some(v) = value {
                args.push(format!("--{prefix}-{suffix}"));
                args.push(v);
            }
        };
        put("line1", self.line1);
        put("line2", self.line2);
        put("city", self.city);
        put("state", self.state);
        put("postal-code", self.postal_code);
        put("country", self.country);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_address_appends_nothing() {
        let mut args = vec!["customers".to_string()];
        AddressArgs::default().push(&mut args, "billing");
        assert_eq!(args, vec!["customers".to_string()]);
    }

    #[test]
    fn partial_address_appends_only_set_flags_under_the_prefix() {
        let mut args = Vec::new();
        AddressArgs {
            city: Some("Denver".into()),
            country: Some("US".into()),
            ..Default::default()
        }
        .push(&mut args, "shipping");
        assert_eq!(
            args,
            vec!["--shipping-city", "Denver", "--shipping-country", "US"]
        );
    }

    #[test]
    fn full_address_keeps_cli_flag_order() {
        let mut args = Vec::new();
        AddressArgs {
            line1: Some("123 Test St".into()),
            line2: Some("Apt 4".into()),
            city: Some("Denver".into()),
            state: Some("CO".into()),
            postal_code: Some("80202".into()),
            country: Some("US".into()),
        }
        .push(&mut args, "billing");
        assert_eq!(
            args,
            vec![
                "--billing-line1",
                "123 Test St",
                "--billing-line2",
                "Apt 4",
                "--billing-city",
                "Denver",
                "--billing-state",
                "CO",
                "--billing-postal-code",
                "80202",
                "--billing-country",
                "US",
            ]
        );
    }
}

#![cfg(feature = "properties")]

use rstest::rstest;
use serde::Deserialize;
use serde_saphyr::{Options, PropertySyntax, from_str_with_options};
use std::collections::BTreeMap;

const VARS: &[(&str, &str)] = &[
    ("SET", "value"),
    ("EMPTY", ""),
    ("NUL", "null"),
    ("TILDE", "~"),
    ("NUL_CAPS", "NULL"),
    ("PORT", "5432"),
    ("FLAG", "true"),
];

fn options(syntax: PropertySyntax) -> Options {
    serde_saphyr::options! {
        property_syntax: syntax,
    }
    .with_properties(
        VARS.iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
    )
}

fn with_nulls() -> Options {
    options(PropertySyntax::DockerComposeWithNulls)
}

#[derive(Debug, Deserialize, PartialEq)]
struct Config<T> {
    value: T,
}

fn load<T: for<'de> Deserialize<'de>>(yaml: &str, options: Options) -> Result<T, String> {
    from_str_with_options::<Config<T>>(yaml, options)
        .map(|config| config.value)
        .map_err(|err| err.to_string())
}

#[rstest]
// A value that is solely a reference to an unset property is null, whatever its style.
#[case::bare_unset("value: $MISSING\n", None)]
#[case::braced_unset("value: ${MISSING}\n", None)]
#[case::single_quoted_unset("value: '$MISSING'\n", None)]
#[case::double_quoted_unset("value: \"${MISSING}\"\n", None)]
#[case::stripped_block_unset("value: |-\n  $MISSING\n", None)]
#[case::folded_stripped_unset("value: >-\n  $MISSING\n", None)]
// A clipped block keeps its newline, so it is not a sole reference.
#[case::clipped_block_unset("value: |\n  $MISSING\n", Some("\n"))]
// An explicitly empty property is an empty string.
#[case::bare_empty("value: $EMPTY\n", Some(""))]
#[case::braced_empty("value: ${EMPTY}\n", Some(""))]
#[case::quoted_empty("value: '$EMPTY'\n", Some(""))]
// Expanded null markers are null.
#[case::expanded_null("value: $NUL\n", None)]
#[case::expanded_tilde("value: ${TILDE}\n", None)]
#[case::expanded_null_caps("value: $NUL_CAPS\n", None)]
#[case::quoted_expanded_null("value: \"$NUL\"\n", None)]
#[case::default_null("value: ${MISSING:-null}\n", None)]
#[case::default_tilde("value: ${MISSING-~}\n", None)]
// Selected empty defaults stay empty strings.
#[case::nested_unset_default("value: ${A:-${B}}\n", Some(""))]
#[case::empty_default("value: ${MISSING-}\n", Some(""))]
#[case::empty_nonempty_default("value: ${MISSING:-}\n", Some(""))]
#[case::empty_alternative("value: ${MISSING+x}\n", Some(""))]
// Surrounding text is kept.
#[case::prefix_unset("value: prefix-$MISSING\n", Some("prefix-"))]
#[case::suffix_unset("value: ${MISSING}-suffix\n", Some("-suffix"))]
#[case::prefix_null("value: prefix-$NUL\n", Some("prefix-null"))]
#[case::two_unset("value: $MISSING$MISSING\n", Some(""))]
#[case::escaped("value: $$MISSING\n", Some("$MISSING"))]
#[case::set("value: $SET\n", Some("value"))]
// String-forcing tags keep their meaning.
#[case::str_tag_unset("value: !!str $MISSING\n", Some(""))]
#[case::non_specific_tag_unset("value: ! $MISSING\n", Some(""))]
#[case::str_tag_null("value: !!str $NUL\n", Some("null"))]
// Scalars without an expansion resolve as in every other mode.
#[case::literal_null("value: null\n", None)]
#[case::literal_tilde("value: ~\n", None)]
#[case::literal_empty("value:\n", None)]
#[case::quoted_literal_null("value: 'null'\n", Some("null"))]
#[case::quoted_literal_empty("value: ''\n", Some(""))]
fn option_string(#[case] yaml: &str, #[case] expected: Option<&str>) {
    let value: Option<String> = load(yaml, with_nulls()).unwrap();
    assert_eq!(value.as_deref(), expected, "{yaml}");
}

#[rstest]
#[case::unset("value: $MISSING\n", serde_json::Value::Null)]
#[case::expanded_null("value: $NUL\n", serde_json::Value::Null)]
#[case::empty("value: $EMPTY\n", serde_json::json!(""))]
// Expanded numbers and booleans stay strings for generic targets, as in `DockerCompose`.
#[case::number("value: $PORT\n", serde_json::json!("5432"))]
#[case::boolean("value: $FLAG\n", serde_json::json!("true"))]
#[case::literal_number("value: 5432\n", serde_json::json!(5432))]
fn untyped(#[case] yaml: &str, #[case] expected: serde_json::Value) {
    let value: serde_json::Value = load(yaml, with_nulls()).unwrap();
    assert_eq!(value, expected, "{yaml}");
}

#[test]
fn typed_conversions_are_unchanged() {
    assert_eq!(load::<u16>("value: $PORT\n", with_nulls()), Ok(5432));
    assert_eq!(
        load::<Option<u16>>("value: $PORT\n", with_nulls()),
        Ok(Some(5432))
    );
    assert_eq!(
        load::<Option<u16>>("value: $MISSING\n", with_nulls()),
        Ok(None)
    );
    assert_eq!(load::<bool>("value: \"$FLAG\"\n", with_nulls()), Ok(true));
    assert_eq!(
        load::<Option<bool>>("value: $MISSING\n", with_nulls()),
        Ok(None)
    );
    assert_eq!(
        load::<Option<bool>>("value: $NUL\n", with_nulls()),
        Ok(None)
    );
}

/// A value that becomes null behaves exactly like a literal null in the same position.
#[rstest]
#[case::unset("value: $MISSING\n")]
#[case::expanded_null("value: $NUL\n")]
fn null_into_non_option_matches_literal_null(#[case] yaml: &str) {
    let literal = load::<String>("value: null\n", with_nulls());
    let expanded = load::<String>(yaml, with_nulls());
    assert_eq!(
        expanded.is_ok(),
        literal.is_ok(),
        "{expanded:?} vs {literal:?}"
    );
    assert!(load::<bool>(yaml, with_nulls()).is_err());
    assert!(load::<u16>(yaml, with_nulls()).is_err());
}

#[rstest]
#[case::unset_or_empty_unset("value: ${MISSING:?must be set}\n")]
#[case::unset_or_empty_empty("value: ${EMPTY:?must be set}\n")]
#[case::unset_only("value: ${MISSING?must be set}\n")]
fn required_references_still_error(#[case] yaml: &str) {
    let err = load::<Option<String>>(yaml, with_nulls()).unwrap_err();
    assert!(err.contains("must be set"), "{err}");
    assert!(err.contains("line 1 column 8"), "{err}");
}

#[test]
fn aliases_resolve_to_the_same_null() {
    let parsed: BTreeMap<String, Option<String>> =
        from_str_with_options("a: &x $MISSING\nb: *x\nc: &y $EMPTY\nd: *y\n", with_nulls())
            .unwrap();
    assert_eq!(parsed["a"], None);
    assert_eq!(parsed["b"], None);
    assert_eq!(parsed["c"].as_deref(), Some(""));
    assert_eq!(parsed["d"].as_deref(), Some(""));
}

#[test]
fn keys_are_not_interpolated() {
    let parsed: BTreeMap<String, Option<String>> =
        from_str_with_options("$MISSING: $MISSING\n$NUL: x\n", with_nulls()).unwrap();
    assert_eq!(parsed.get("$MISSING"), Some(&None));
    assert_eq!(parsed.get("$NUL").cloned().flatten().as_deref(), Some("x"));
}

#[test]
fn values_in_nested_collections_become_null() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Nested {
        list: Vec<Option<String>>,
        map: BTreeMap<String, Option<u16>>,
    }
    let parsed: Nested = from_str_with_options(
        "list: [$MISSING, $EMPTY, $SET]\nmap:\n  port: $PORT\n  other: ${MISSING}\n",
        with_nulls(),
    )
    .unwrap();
    assert_eq!(
        parsed.list,
        [None, Some(String::new()), Some("value".into())]
    );
    assert_eq!(parsed.map["port"], Some(5432));
    assert_eq!(parsed.map["other"], None);
}

/// The plain Compose mode keeps its string-preserving behavior.
#[rstest]
#[case::unset("value: $MISSING\n", Some(""))]
#[case::expanded_null("value: $NUL\n", Some("null"))]
#[case::expanded_tilde("value: $TILDE\n", Some("~"))]
#[case::empty("value: $EMPTY\n", Some(""))]
fn docker_compose_is_unchanged(#[case] yaml: &str, #[case] expected: Option<&str>) {
    let value: Option<String> = load(yaml, options(PropertySyntax::DockerCompose)).unwrap();
    assert_eq!(value.as_deref(), expected, "{yaml}");
}

#[test]
fn existing_variants_keep_their_integer_values() {
    assert_eq!(PropertySyntax::Braced as u8, 0);
    assert_eq!(PropertySyntax::BracedOrBare as u8, 1);
    assert_eq!(PropertySyntax::DockerCompose as u8, 2);
    assert_eq!(PropertySyntax::DockerComposeWithNulls as u8, 3);
}

#[test]
fn works_without_a_property_map() {
    let options = serde_saphyr::options! {
        property_syntax: PropertySyntax::DockerComposeWithNulls,
    };
    assert_eq!(
        load::<Option<String>>("value: $MISSING\n", options.clone()),
        Ok(None)
    );
    assert_eq!(
        load::<Option<String>>("value: a-$MISSING\n", options),
        Ok(Some("a-".to_owned()))
    );
}

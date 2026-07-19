//! Sensitive-column detection.
//!
//! v1 is rule-based: normalized column-name patterns gated by canonical
//! type category. The `Detector` trait exists so an LLM-assisted detector
//! can be added as a second implementation later — the rule engine stays
//! as the fast first pass the LLM refines, not something it replaces.

use ddbcore::{Table, TableRef, TypeCategory};
use serde::{Deserialize, Serialize};

/// What kind of sensitive data a column is believed to hold.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PiiKind {
    Email,
    Phone,
    PersonName,
    Address,
    GovernmentId,
    CreditCard,
    Credential,
    DateOfBirth,
    IpAddress,
    Custom(String),
}

/// One detected sensitive column, with a confidence in `[0.0, 1.0]` and a
/// human-readable reason. Every finding is reviewable and overridable —
/// detection recommends, the user decides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub table: TableRef,
    pub column: String,
    pub kind: PiiKind,
    pub confidence: f32,
    pub reason: String,
}

pub trait Detector {
    fn detect_table(&self, table: &Table) -> Vec<Finding>;
}

/// Whether a rule applies to a given canonical type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeGate {
    /// Char/VarChar/Text only.
    Stringy,
    /// Stringy or any integer/decimal (ids and numbers stored either way).
    StringyOrNumeric,
    /// Date/Timestamp or stringy (dates get stored as text often enough).
    Datey,
}

fn gate_matches(gate: TypeGate, category: &TypeCategory) -> bool {
    let stringy = matches!(
        category,
        TypeCategory::Char { .. } | TypeCategory::VarChar { .. } | TypeCategory::Text
    );
    match gate {
        TypeGate::Stringy => stringy,
        TypeGate::StringyOrNumeric => {
            stringy
                || matches!(
                    category,
                    TypeCategory::SmallInt
                        | TypeCategory::Integer
                        | TypeCategory::BigInt
                        | TypeCategory::Decimal { .. }
                )
        }
        TypeGate::Datey => {
            stringy || matches!(category, TypeCategory::Date | TypeCategory::Timestamp { .. })
        }
    }
}

struct Rule {
    kind: PiiKind,
    gate: TypeGate,
    /// Normalized name must EQUAL one of these → high confidence.
    exact: &'static [&'static str],
    /// Normalized name must CONTAIN one of these → medium confidence.
    contains: &'static [&'static str],
}

/// Normalizes a column name for matching: lowercase, separators stripped.
/// `Customer_Email`, `customerEmail`, and `customer-email` all become
/// `customeremail`.
fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn rules() -> &'static [Rule] {
    const RULES: &[Rule] = &[
        Rule {
            kind: PiiKind::Email,
            gate: TypeGate::Stringy,
            exact: &["email", "emailaddress", "mail"],
            contains: &["email"],
        },
        Rule {
            kind: PiiKind::Phone,
            gate: TypeGate::StringyOrNumeric,
            exact: &["phone", "phonenumber", "telephone", "mobile", "cell", "fax"],
            contains: &["phone", "mobile", "telephone"],
        },
        Rule {
            kind: PiiKind::PersonName,
            gate: TypeGate::Stringy,
            exact: &[
                "firstname", "lastname", "middlename", "fullname", "surname", "givenname",
                "familyname", "maidenname", "nickname", "displayname",
            ],
            contains: &["firstname", "lastname", "fullname", "surname", "givenname", "familyname"],
        },
        Rule {
            kind: PiiKind::Address,
            gate: TypeGate::Stringy,
            exact: &[
                "address", "street", "streetaddress", "addressline1", "addressline2", "city",
                "postcode", "postalcode", "zipcode", "zip",
            ],
            contains: &["address", "street", "postcode", "postalcode", "zipcode"],
        },
        Rule {
            kind: PiiKind::GovernmentId,
            gate: TypeGate::StringyOrNumeric,
            exact: &[
                "ssn", "socialsecuritynumber", "nationalid", "taxid", "tin", "nino", "passport",
                "passportnumber", "driverslicense", "driverslicence", "licensenumber",
            ],
            contains: &["ssn", "socialsecurity", "nationalid", "taxid", "passport", "driverslicen"],
        },
        Rule {
            kind: PiiKind::CreditCard,
            gate: TypeGate::StringyOrNumeric,
            exact: &["creditcard", "creditcardnumber", "cardnumber", "ccnumber", "pan", "iban", "accountnumber"],
            contains: &["creditcard", "cardnumber", "ccnum", "iban"],
        },
        Rule {
            kind: PiiKind::Credential,
            gate: TypeGate::Stringy,
            exact: &[
                "password", "passwordhash", "passwd", "secret", "apikey", "apisecret", "token",
                "accesstoken", "refreshtoken", "privatekey", "sessionid",
            ],
            contains: &["password", "secret", "apikey", "token", "privatekey"],
        },
        Rule {
            kind: PiiKind::DateOfBirth,
            gate: TypeGate::Datey,
            exact: &["dob", "dateofbirth", "birthdate", "birthday", "borndate"],
            contains: &["dateofbirth", "birthdate", "birthday"],
        },
        Rule {
            kind: PiiKind::IpAddress,
            gate: TypeGate::Stringy,
            exact: &["ip", "ipaddress", "ipaddr", "clientip", "remoteip"],
            contains: &["ipaddress", "clientip", "remoteip"],
        },
    ];
    RULES
}

/// Column-name + type-category rule matching. No data is read — this
/// operates purely on reflected schema, so it is safe to run against a
/// production catalog before any row has been touched.
pub struct RuleBasedDetector;

impl Detector for RuleBasedDetector {
    fn detect_table(&self, table: &Table) -> Vec<Finding> {
        let mut findings = Vec::new();
        for column in &table.columns {
            let normalized = normalize(&column.name);
            if normalized.is_empty() {
                continue;
            }
            // First matching rule wins, exact matches beat contains
            // matches across all rules.
            let mut best: Option<(PiiKind, f32, String)> = None;
            for rule in rules() {
                if !gate_matches(rule.gate, &column.data_type.category) {
                    continue;
                }
                if rule.exact.iter().any(|e| *e == normalized) {
                    let candidate = (
                        rule.kind.clone(),
                        0.95,
                        format!("column name '{}' exactly matches a known {} pattern", column.name, kind_label(&rule.kind)),
                    );
                    if best.as_ref().map(|(_, c, _)| *c < 0.95).unwrap_or(true) {
                        best = Some(candidate);
                    }
                } else if rule.contains.iter().any(|c| normalized.contains(c)) {
                    if best.is_none() {
                        best = Some((
                            rule.kind.clone(),
                            0.7,
                            format!("column name '{}' contains a known {} pattern", column.name, kind_label(&rule.kind)),
                        ));
                    }
                }
            }
            if let Some((kind, confidence, reason)) = best {
                findings.push(Finding {
                    table: table.table_ref(),
                    column: column.name.clone(),
                    kind,
                    confidence,
                    reason,
                });
            }
        }
        findings
    }
}

pub fn kind_label(kind: &PiiKind) -> &str {
    match kind {
        PiiKind::Email => "email",
        PiiKind::Phone => "phone number",
        PiiKind::PersonName => "person name",
        PiiKind::Address => "address",
        PiiKind::GovernmentId => "government ID",
        PiiKind::CreditCard => "payment card / account number",
        PiiKind::Credential => "credential",
        PiiKind::DateOfBirth => "date of birth",
        PiiKind::IpAddress => "IP address",
        PiiKind::Custom(name) => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddbcore::{Column, DataType, Table};

    fn col(name: &str, category: TypeCategory) -> Column {
        Column {
            name: name.into(),
            ordinal_position: 1,
            data_type: DataType { category, native_type: String::new() },
            nullable: true,
            default: None,
            is_identity: false,
            identity_generation: None,
            comment: None,
        }
    }

    fn table(columns: Vec<Column>) -> Table {
        Table {
            schema: "public".into(),
            name: "customers".into(),
            columns,
            primary_key: None,
            foreign_keys: vec![],
            unique_constraints: vec![],
            check_constraints: vec![],
            exclusion_constraints: vec![],
            indexes: vec![],
            triggers: vec![],
            comment: None,
            partition_key: None,
            partition_parent: None,
        }
    }

    #[test]
    fn detects_exact_email_with_high_confidence() {
        let t = table(vec![col("email", TypeCategory::VarChar { length: Some(255) })]);
        let findings = RuleBasedDetector.detect_table(&t);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, PiiKind::Email);
        assert!(findings[0].confidence >= 0.9);
    }

    #[test]
    fn detects_camel_and_snake_case_variants() {
        let t = table(vec![
            col("Customer_Email", TypeCategory::Text),
            col("phoneNumber", TypeCategory::VarChar { length: Some(20) }),
            col("first_name", TypeCategory::Text),
        ]);
        let findings = RuleBasedDetector.detect_table(&t);
        let kinds: Vec<&PiiKind> = findings.iter().map(|f| &f.kind).collect();
        assert!(kinds.contains(&&PiiKind::Email));
        assert!(kinds.contains(&&PiiKind::Phone));
        assert!(kinds.contains(&&PiiKind::PersonName));
    }

    #[test]
    fn type_gate_blocks_numeric_email() {
        // An integer column named "email" is almost certainly not an
        // email address; the type gate must suppress the match.
        let t = table(vec![col("email", TypeCategory::Integer)]);
        assert!(RuleBasedDetector.detect_table(&t).is_empty());
    }

    #[test]
    fn detects_dob_on_date_column_only_with_matching_name() {
        let t = table(vec![
            col("date_of_birth", TypeCategory::Date),
            col("created_at", TypeCategory::Timestamp { precision: None, with_timezone: true }),
        ]);
        let findings = RuleBasedDetector.detect_table(&t);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, PiiKind::DateOfBirth);
    }

    #[test]
    fn ignores_unremarkable_columns() {
        let t = table(vec![
            col("id", TypeCategory::BigInt),
            col("status", TypeCategory::Text),
            col("total", TypeCategory::Decimal { precision: Some(10), scale: Some(2) }),
        ]);
        assert!(RuleBasedDetector.detect_table(&t).is_empty());
    }
}

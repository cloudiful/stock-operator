use anyhow::Result;
use schemars::JsonSchema;

use super::types::{
    BusinessTable, ExecutionRecord, ExecutionStructuredSnapshot, FundsRecord,
    FundsStructuredSnapshot, OrderRecord, OrderStructuredSnapshot, PositionRecord,
    PositionStructuredSnapshot, StructuredField,
};
use crate::pages::{
    reader::PageReader,
    types::{DataQuality, ObservedText, TableColumn, TableRow},
};

impl PageReader {
    pub fn positions_structured(&self) -> Result<PositionStructuredSnapshot> {
        let snapshot = self.positions_ocr()?;
        let (data, quality) = position_table(&snapshot.data.columns, &snapshot.data.rows);
        Ok(crate::pages::types::snapshot_with_source(
            snapshot.workspace,
            snapshot.panel,
            data,
            quality.min(snapshot.quality),
            snapshot.source,
            snapshot.warnings,
        ))
    }

    pub fn orders_structured(&self) -> Result<OrderStructuredSnapshot> {
        let snapshot = self.orders_ocr()?;
        let (data, quality) = order_table(&snapshot.data.columns, &snapshot.data.rows);
        Ok(crate::pages::types::snapshot_with_source(
            snapshot.workspace,
            snapshot.panel,
            data,
            quality.min(snapshot.quality),
            snapshot.source,
            snapshot.warnings,
        ))
    }

    pub fn executions_structured(&self) -> Result<ExecutionStructuredSnapshot> {
        let snapshot = self.executions_ocr()?;
        let (data, quality) = execution_table(&snapshot.data.columns, &snapshot.data.rows);
        Ok(crate::pages::types::snapshot_with_source(
            snapshot.workspace,
            snapshot.panel,
            data,
            quality.min(snapshot.quality),
            snapshot.source,
            snapshot.warnings,
        ))
    }

    pub fn funds_structured(&self) -> Result<FundsStructuredSnapshot> {
        let snapshot = self.funds_ocr()?;
        let (data, quality) = funds_table(&snapshot.data.columns, &snapshot.data.rows);
        Ok(crate::pages::types::snapshot_with_source(
            snapshot.workspace,
            snapshot.panel,
            data,
            quality.min(snapshot.quality),
            snapshot.source,
            snapshot.warnings,
        ))
    }
}

#[derive(Clone)]
struct ColumnMap {
    titles: Vec<String>,
}

impl ColumnMap {
    fn new(columns: &[TableColumn]) -> Self {
        Self {
            titles: columns
                .iter()
                .map(|column| column.title.value.clone().unwrap_or_default())
                .collect(),
        }
    }

    fn index(&self, aliases: &[&str]) -> Option<usize> {
        aliases
            .iter()
            .find_map(|alias| self.titles.iter().position(|title| title == alias))
            .or_else(|| {
                aliases.iter().find_map(|alias| {
                    self.titles
                        .iter()
                        .position(|title| !title.is_empty() && title.contains(alias))
                })
            })
    }

    fn field(&self, row: &TableRow, aliases: &[&str], numeric: bool) -> StructuredField {
        let Some(index) = self.index(aliases) else {
            return StructuredField::unavailable();
        };
        let Some(cell) = row.cells.get(index) else {
            return StructuredField::unavailable();
        };
        normalize_field(cell, numeric)
    }

    fn observed<'a>(&self, row: &'a TableRow, aliases: &[&str]) -> Option<&'a ObservedText> {
        self.index(aliases).and_then(|index| row.cells.get(index))
    }

    fn numeric_pair(
        &self,
        row: &TableRow,
        left_aliases: &[&str],
        right_aliases: &[&str],
    ) -> (StructuredField, StructuredField) {
        let left = self.field(row, left_aliases, true);
        let right = self.field(row, right_aliases, true);
        if left.normalized.is_some() && right.normalized.is_some() {
            return (left, right);
        }
        if let Some((merged_left, merged_right)) = left.raw.as_deref().and_then(split_numeric_pair)
            && right
                .normalized
                .as_deref()
                .is_none_or(|value| value == merged_right)
        {
            let right = if right.normalized.is_some() {
                right
            } else {
                StructuredField::inferred(merged_right)
            };
            return (StructuredField::inferred(merged_left), right);
        }
        if let Some((merged_left, merged_right)) = right.raw.as_deref().and_then(split_numeric_pair)
            && left
                .normalized
                .as_deref()
                .is_none_or(|value| value == merged_left)
        {
            let left = if left.normalized.is_some() {
                left
            } else {
                StructuredField::inferred(merged_left)
            };
            return (left, StructuredField::inferred(merged_right));
        }
        (left, right)
    }

    fn security_fields(&self, row: &TableRow) -> (StructuredField, StructuredField) {
        let code = self.observed(row, &["证券代码"]);
        let name = self.field(row, &["证券名称"], false);
        if let Some(code) = code.filter(|value| value.value.is_some()) {
            return split_security_observation(code, name, false);
        }
        if let Some(name_observation) = self
            .observed(row, &["证券名称"])
            .filter(|value| value.value.as_deref().is_some_and(contains_security_code))
        {
            return split_security_observation(
                name_observation,
                StructuredField::unavailable(),
                true,
            );
        }
        (StructuredField::unavailable(), name)
    }
}

fn split_security_observation(
    observed: &ObservedText,
    fallback_name: StructuredField,
    inferred_code: bool,
) -> (StructuredField, StructuredField) {
    let raw = observed.value.as_deref().expect("caller checked value");
    let parts = raw.split_whitespace().collect::<Vec<_>>();
    let Some((code_index, security_code)) = parts
        .iter()
        .enumerate()
        .find(|(_, value)| is_security_code(value))
    else {
        return (unparseable_field(observed), fallback_name);
    };
    let code_field = if inferred_code {
        StructuredField::inferred((*security_code).to_string())
    } else {
        StructuredField {
            raw: Some((*security_code).to_string()),
            normalized: Some((*security_code).to_string()),
            quality: observed.quality,
            source: observed.source,
        }
    };
    if fallback_name.raw.is_some() {
        return (
            code_field,
            clean_security_name(fallback_name, security_code),
        );
    }
    let inferred_name = parts
        .into_iter()
        .enumerate()
        .filter_map(|(index, value)| (index != code_index).then_some(value))
        .collect::<Vec<_>>()
        .join(" ");
    if inferred_name.is_empty() {
        (code_field, fallback_name)
    } else {
        (code_field, StructuredField::inferred(inferred_name))
    }
}

fn clean_security_name(field: StructuredField, security_code: &str) -> StructuredField {
    let Some(normalized) = field.normalized.as_deref() else {
        return field;
    };
    let mut parts = normalized.split_whitespace();
    if parts.next() != Some(security_code) {
        return field;
    }
    let name = parts.collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        field
    } else {
        StructuredField::inferred(name)
    }
}

fn contains_security_code(raw: &str) -> bool {
    raw.split_whitespace().any(is_security_code)
}

fn normalize_field(observed: &ObservedText, numeric: bool) -> StructuredField {
    let Some(raw) = observed.value.clone() else {
        return StructuredField::unavailable();
    };
    let normalized = if numeric {
        normalize_number(&raw)
    } else {
        Some(raw.trim().to_string())
    };
    let quality = if normalized.is_some() {
        observed.quality
    } else {
        DataQuality::Partial
    };
    StructuredField {
        raw: Some(raw),
        normalized,
        quality,
        source: observed.source,
    }
}

fn unparseable_field(observed: &ObservedText) -> StructuredField {
    StructuredField {
        raw: observed.value.clone(),
        normalized: None,
        quality: DataQuality::Partial,
        source: observed.source,
    }
}

fn is_security_code(value: &str) -> bool {
    matches!(value.len(), 5 | 6) && value.chars().all(|character| character.is_ascii_digit())
}

fn normalize_number(raw: &str) -> Option<String> {
    let tokens = raw.split_whitespace().collect::<Vec<_>>();
    if tokens.len() != 1 {
        return None;
    }
    let mut token = tokens[0]
        .replace(',', "")
        .replace('．', ".")
        .replace(['－', '−'], "-");
    if token.starts_with('(') && token.ends_with(')') {
        token = format!("-{}", &token[1..token.len() - 1]);
    }
    if token.ends_with('%') {
        token.pop();
    }
    if token.is_empty()
        || token.chars().filter(|character| *character == '.').count() > 1
        || !token
            .chars()
            .all(|character| character.is_ascii_digit() || ".+-".contains(character))
    {
        return None;
    }
    let sign_count = token
        .chars()
        .filter(|character| "+-".contains(*character))
        .count();
    if sign_count > 1 || token[1..].contains(['+', '-']) {
        return None;
    }
    if token.chars().any(|character| character.is_ascii_digit()) {
        Some(token)
    } else {
        None
    }
}

fn split_numeric_pair(raw: &str) -> Option<(String, String)> {
    let values = raw
        .split_whitespace()
        .map(normalize_number)
        .collect::<Option<Vec<_>>>()?;
    (values.len() == 2).then(|| (values[0].clone(), values[1].clone()))
}

fn position_table(
    columns: &[TableColumn],
    rows: &[TableRow],
) -> (BusinessTable<PositionRecord>, DataQuality) {
    let map = ColumnMap::new(columns);
    let mut warnings = Vec::new();
    let records = rows
        .iter()
        .filter(|row| has_data(row))
        .map(|row| {
            let (security_code, security_name) = map.security_fields(row);
            PositionRecord {
                security_code,
                security_name,
                yesterday_balance: map.field(row, &["昨日余额"], true),
                reference_holding: map.field(row, &["参考持股"], true),
                available_quantity: map.field(row, &["可用股份", "可用数量"], true),
                frozen_quantity: map.field(row, &["冻结数量"], true),
                cost_price: map.field(row, &["成本价"], true),
                current_price: map.field(row, &["当前价"], true),
                current_cost: map.field(row, &["当前成本"], true),
                market_value: map.field(row, &["最新市值"], true),
                floating_pnl: map.field(row, &["浮动盈亏"], true),
                pnl_ratio: map.field(row, &["盈亏比例"], true),
                today_buy_quantity: map.field(row, &["今买成交数量"], true),
                reference_pnl: map.field(row, &["无费用参考盈亏"], true),
                share_balance: map.field(row, &["股份余额"], true),
                shareholder_account: map.field(row, &["股东代码"], false),
                custodian_unit: map.field(row, &["托管单元"], false),
                fund_account: map.field(row, &["资金账号"], false),
            }
        })
        .collect::<Vec<_>>();
    let quality = records_quality(&records, &mut warnings, validate_position);
    (
        BusinessTable {
            row_count: records.len(),
            mapped_columns: map.titles,
            records,
            warnings,
        },
        quality,
    )
}

fn order_table(
    columns: &[TableColumn],
    rows: &[TableRow],
) -> (BusinessTable<OrderRecord>, DataQuality) {
    let map = ColumnMap::new(columns);
    let mut warnings = Vec::new();
    let records = rows
        .iter()
        .filter(|row| has_data(row))
        .map(|row| {
            let (security_code, security_name) = map.security_fields(row);
            OrderRecord {
                order_id: map.field(
                    row,
                    &["委托编号", "申请编号", "合同编号", "委托序号"],
                    false,
                ),
                submitted_at: map.field(row, &["委托时间", "申报时间"], false),
                security_code,
                security_name,
                side: map.field(row, &["买卖方向", "买卖标志", "操作"], false),
                order_type: map.field(row, &["委托类型", "报价方式"], false),
                price: map.field(row, &["委托价格", "委托价"], true),
                quantity: map.field(row, &["委托数量", "申报数量"], true),
                filled_quantity: map.field(row, &["成交数量", "已成交"], true),
                remaining_quantity: map.field(row, &["剩余数量", "未成交"], true),
                status: map.field(row, &["委托状态", "状态"], false),
                cancelable: map.field(row, &["可撤", "可撤单", "撤单标志"], false),
            }
        })
        .collect::<Vec<_>>();
    let quality = records_quality(&records, &mut warnings, validate_order);
    (
        BusinessTable {
            row_count: records.len(),
            mapped_columns: map.titles,
            records,
            warnings,
        },
        quality,
    )
}

fn execution_table(
    columns: &[TableColumn],
    rows: &[TableRow],
) -> (BusinessTable<ExecutionRecord>, DataQuality) {
    let map = ColumnMap::new(columns);
    let mut warnings = Vec::new();
    let records = rows
        .iter()
        .filter(|row| has_data(row))
        .map(|row| {
            let (security_code, security_name) = map.security_fields(row);
            let merged_price = map.observed(row, &["成交价格", "成交价"]);
            let merged_amount = map.observed(row, &["成交金额", "成交额"]);
            let merged_order_type = map.observed(row, &["委托类型", "报价方式"]);
            let merged_business = map.observed(row, &["业务名称"]);
            let side = map.field(row, &["买卖方向", "买卖标志", "操作"], false);
            let price = map.field(row, &["成交价格", "成交价"], true);
            let quantity = map.field(row, &["成交数量", "成交股数"], true);
            let amount = map.field(row, &["成交金额", "成交额"], true);
            let price = price_from_merged(price, merged_price);
            let recovered_pair = execution_pair(merged_amount, price.normalized.as_deref());
            ExecutionRecord {
                execution_id: map.field(row, &["成交编号", "成交序号"], false),
                executed_at: map.field(row, &["成交时间", "成交日期"], false),
                order_id: map.field(row, &["委托编号", "申请编号", "合同编号"], false),
                security_code,
                security_name,
                side: side_from_merged(side, [merged_price, merged_order_type, merged_business]),
                price,
                quantity: if quantity.normalized.is_some() {
                    quantity
                } else {
                    recovered_pair
                        .as_ref()
                        .map(|(quantity, _)| StructuredField::inferred(quantity.clone()))
                        .unwrap_or(quantity)
                },
                amount: if amount.normalized.is_some() {
                    amount
                } else {
                    recovered_pair
                        .map(|(_, amount)| StructuredField::inferred(amount))
                        .unwrap_or(amount)
                },
            }
        })
        .collect::<Vec<_>>();
    let quality = records_quality(&records, &mut warnings, validate_execution);
    (
        BusinessTable {
            row_count: records.len(),
            mapped_columns: map.titles,
            records,
            warnings,
        },
        quality,
    )
}

fn side_from_merged(field: StructuredField, merged: [Option<&ObservedText>; 3]) -> StructuredField {
    if field.normalized.is_some() {
        return field;
    }
    merged
        .into_iter()
        .flatten()
        .filter_map(|value| value.value.as_deref())
        .find_map(|value| {
            value
                .split_whitespace()
                .find(|part| matches!(*part, "买入" | "卖出"))
        })
        .map(|value| StructuredField::inferred(value.to_string()))
        .unwrap_or(field)
}

fn price_from_merged(field: StructuredField, merged: Option<&ObservedText>) -> StructuredField {
    if field.normalized.is_some() {
        return field;
    }
    merged
        .and_then(|value| value.value.as_deref())
        .and_then(|value| {
            let numbers = value
                .split_whitespace()
                .filter_map(normalize_number)
                .collect::<Vec<_>>();
            (numbers.len() == 1).then(|| numbers[0].clone())
        })
        .map(StructuredField::inferred)
        .unwrap_or(field)
}

fn execution_pair(
    merged: Option<&ObservedText>,
    normalized_price: Option<&str>,
) -> Option<(String, String)> {
    let pair = merged
        .and_then(|value| value.value.as_deref())
        .and_then(|value| {
            let numbers = value
                .split_whitespace()
                .map(normalize_number)
                .collect::<Option<Vec<_>>>()?;
            (numbers.len() == 2).then(|| (numbers[0].clone(), numbers[1].clone()))
        })?;
    let price = normalized_price?.parse::<f64>().ok()?;
    let quantity = pair.0.parse::<f64>().ok()?;
    let amount = pair.1.parse::<f64>().ok()?;
    ((price * quantity - amount).abs() <= 0.02).then_some(pair)
}

fn funds_table(
    columns: &[TableColumn],
    rows: &[TableRow],
) -> (BusinessTable<FundsRecord>, DataQuality) {
    let map = ColumnMap::new(columns);
    let mut warnings = Vec::new();
    let records = rows
        .iter()
        .filter(|row| has_data(row))
        .map(|row| {
            let (frozen_funds, market_value) = map.numeric_pair(
                row,
                &["冻结资金", "冻结金额"],
                &["证券市值", "股票市值", "市值"],
            );
            let (cash_assets, total_assets) = map.numeric_pair(row, &["现金资产"], &["总资产"]);
            let cash_balance = {
                let dedicated = map.field(row, &["资金余额", "现金余额"], true);
                if dedicated.normalized.is_some() {
                    dedicated
                } else {
                    cash_assets
                }
            };
            FundsRecord {
                total_assets,
                available_funds: map.field(row, &["可用资金"], true),
                frozen_funds,
                cash_balance,
                market_value,
                withdrawable_funds: map.field(row, &["可取资金", "可取余额"], true),
            }
        })
        .collect::<Vec<_>>();
    let quality = records_quality(&records, &mut warnings, validate_funds);
    (
        BusinessTable {
            row_count: records.len(),
            mapped_columns: map.titles,
            records,
            warnings,
        },
        quality,
    )
}

fn has_data(row: &TableRow) -> bool {
    row.cells.iter().any(|cell| {
        cell.value
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty())
    })
}

fn records_quality<T: JsonSchema>(
    records: &[T],
    warnings: &mut Vec<String>,
    validate: impl Fn(&T, usize, &mut Vec<String>) -> bool,
) -> DataQuality {
    if records.is_empty() {
        warnings.push("no structured records were produced".to_string());
        DataQuality::Unavailable
    } else if records
        .iter()
        .enumerate()
        .map(|(index, record)| validate(record, index, warnings))
        .fold(true, |valid, record_valid| valid & record_valid)
    {
        DataQuality::Exact
    } else {
        DataQuality::Partial
    }
}

fn required_field(
    field: &StructuredField,
    label: &str,
    row_index: usize,
    warnings: &mut Vec<String>,
) -> bool {
    if field.normalized.is_some() && field.quality != DataQuality::Unavailable {
        true
    } else {
        warnings.push(format!(
            "required structured field is unavailable at record {row_index}: {label}"
        ));
        false
    }
}

fn validate_position(
    record: &PositionRecord,
    row_index: usize,
    warnings: &mut Vec<String>,
) -> bool {
    [
        required_field(
            &record.security_code,
            "position.security_code",
            row_index,
            warnings,
        ),
        required_field(
            &record.available_quantity,
            "position.available_quantity",
            row_index,
            warnings,
        ),
        required_field(
            &record.current_price,
            "position.current_price",
            row_index,
            warnings,
        ),
    ]
    .into_iter()
    .all(|valid| valid)
}

fn validate_order(record: &OrderRecord, row_index: usize, warnings: &mut Vec<String>) -> bool {
    [
        required_field(
            &record.security_code,
            "order.security_code",
            row_index,
            warnings,
        ),
        required_field(&record.side, "order.side", row_index, warnings),
        required_field(&record.price, "order.price", row_index, warnings),
        required_field(&record.quantity, "order.quantity", row_index, warnings),
        required_field(&record.status, "order.status", row_index, warnings),
    ]
    .into_iter()
    .all(|valid| valid)
}

fn validate_execution(
    record: &ExecutionRecord,
    row_index: usize,
    warnings: &mut Vec<String>,
) -> bool {
    [
        required_field(
            &record.security_code,
            "execution.security_code",
            row_index,
            warnings,
        ),
        required_field(&record.side, "execution.side", row_index, warnings),
        required_field(&record.price, "execution.price", row_index, warnings),
        required_field(&record.quantity, "execution.quantity", row_index, warnings),
    ]
    .into_iter()
    .all(|valid| valid)
}

fn validate_funds(record: &FundsRecord, row_index: usize, warnings: &mut Vec<String>) -> bool {
    [
        required_field(
            &record.available_funds,
            "funds.available_funds",
            row_index,
            warnings,
        ),
        required_field(
            &record.total_assets,
            "funds.total_assets",
            row_index,
            warnings,
        ),
    ]
    .into_iter()
    .all(|valid| valid)
}

trait QualityMin {
    fn min(self, other: DataQuality) -> DataQuality;
}

impl QualityMin for DataQuality {
    fn min(self, other: DataQuality) -> DataQuality {
        use DataQuality::{Exact, Partial, Unavailable};
        match (self, other) {
            (Unavailable, _) | (_, Unavailable) => Unavailable,
            (Partial, _) | (_, Partial) => Partial,
            (Exact, Exact) => Exact,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{execution_table, funds_table, normalize_number, order_table, position_table};
    use crate::pages::types::{DataQuality, DataSource, ObservedText, TableColumn, TableRow};

    #[test]
    fn normalizes_numeric_display_values() {
        assert_eq!(normalize_number("1,234.50"), Some("1234.50".to_string()));
        assert_eq!(normalize_number("(12.30)"), Some("-12.30".to_string()));
        assert_eq!(normalize_number("0.210%"), Some("0.210".to_string()));
    }

    #[test]
    fn rejects_merged_or_non_numeric_values() {
        assert_eq!(normalize_number("4595.05 4545.000"), None);
        assert_eq!(normalize_number("--"), None);
    }

    #[test]
    fn normalizes_unicode_minus_signs() {
        assert_eq!(normalize_number("－12.30"), Some("-12.30".to_string()));
        assert_eq!(normalize_number("−12.30"), Some("-12.30".to_string()));
    }

    #[test]
    fn splits_merged_security_code_and_name() {
        let columns = position_columns();
        let rows = vec![TableRow {
            ordinal: 0,
            cells: vec![
                exact("600028 中国石化"),
                unavailable(),
                exact("900.00"),
                exact("5.0500"),
            ],
        }];

        let (table, quality) = position_table(&columns, &rows);
        let record = &table.records[0];
        assert_eq!(record.security_code.normalized.as_deref(), Some("600028"));
        assert_eq!(record.security_name.normalized.as_deref(), Some("中国石化"));
        assert_eq!(record.security_name.source, DataSource::Inferred);
        assert_eq!(quality, DataQuality::Exact);
    }

    #[test]
    fn strips_security_code_from_merged_name_when_code_column_is_present() {
        let columns = position_columns();
        let rows = vec![TableRow {
            ordinal: 0,
            cells: vec![
                exact("600028 中国石化"),
                partial("600028 中国石化"),
                exact("900.00"),
                exact("5.0500"),
            ],
        }];

        let (table, _) = position_table(&columns, &rows);
        assert_eq!(
            table.records[0].security_name.normalized.as_deref(),
            Some("中国石化")
        );
        assert_eq!(table.records[0].security_name.source, DataSource::Inferred);
    }

    #[test]
    fn recovers_security_code_merged_into_name_column() {
        let columns = position_columns();
        let rows = vec![TableRow {
            ordinal: 0,
            cells: vec![
                unavailable(),
                partial("600028 中国石化"),
                exact("900.00"),
                exact("5.0500"),
            ],
        }];

        let (table, quality) = position_table(&columns, &rows);
        let record = &table.records[0];
        assert_eq!(record.security_code.normalized.as_deref(), Some("600028"));
        assert_eq!(record.security_code.source, DataSource::Inferred);
        assert_eq!(record.security_code.quality, DataQuality::Partial);
        assert_eq!(record.security_name.normalized.as_deref(), Some("中国石化"));
        assert_eq!(quality, DataQuality::Exact);
    }

    #[test]
    fn marks_missing_required_position_fields_partial() {
        let columns = position_columns();
        let rows = vec![TableRow {
            ordinal: 0,
            cells: vec![
                exact("600028 中国石化"),
                unavailable(),
                exact("900.00"),
                unavailable(),
            ],
        }];

        let (table, quality) = position_table(&columns, &rows);
        assert_eq!(quality, DataQuality::Partial);
        assert!(
            table
                .warnings
                .iter()
                .any(|warning| warning.contains("position.current_price"))
        );
    }

    #[test]
    fn reports_each_invalid_record() {
        let columns = position_columns();
        let rows = ["600028 中国石化", "600546 山煤国际"]
            .into_iter()
            .enumerate()
            .map(|(ordinal, code)| TableRow {
                ordinal,
                cells: vec![exact(code), unavailable(), exact("900.00"), unavailable()],
            })
            .collect::<Vec<_>>();

        let (table, quality) = position_table(&columns, &rows);
        assert_eq!(quality, DataQuality::Partial);
        assert_eq!(
            table
                .warnings
                .iter()
                .filter(|warning| warning.contains("position.current_price"))
                .count(),
            2
        );
        assert!(
            table
                .warnings
                .iter()
                .any(|warning| warning.contains("record 1"))
        );
    }

    #[test]
    fn supports_five_digit_security_codes() {
        let columns = position_columns();
        let rows = vec![TableRow {
            ordinal: 0,
            cells: vec![
                exact("00700 腾讯控股"),
                unavailable(),
                exact("100.00"),
                exact("300.00"),
            ],
        }];

        let (table, quality) = position_table(&columns, &rows);
        assert_eq!(quality, DataQuality::Exact);
        assert_eq!(
            table.records[0].security_code.normalized.as_deref(),
            Some("00700")
        );
        assert_eq!(
            table.records[0].security_name.normalized.as_deref(),
            Some("腾讯控股")
        );
    }

    #[test]
    fn leaves_unparseable_security_cell_without_normalized_code() {
        let columns = position_columns();
        let rows = vec![TableRow {
            ordinal: 0,
            cells: vec![
                exact("中国石化"),
                unavailable(),
                exact("900.00"),
                exact("5.0500"),
            ],
        }];

        let (table, quality) = position_table(&columns, &rows);
        assert_eq!(quality, DataQuality::Partial);
        assert_eq!(table.records[0].security_code.normalized, None);
    }

    #[test]
    fn maps_order_execution_and_funds_fixtures() {
        let order_columns = columns(&["证券代码", "买卖方向", "委托价格", "委托数量", "委托状态"]);
        let order_rows = vec![TableRow {
            ordinal: 0,
            cells: vec![
                exact("600028 中国石化"),
                exact("买入"),
                exact("5.0500"),
                exact("100"),
                exact("已报"),
            ],
        }];
        let (orders, order_quality) = order_table(&order_columns, &order_rows);
        assert_eq!(order_quality, DataQuality::Exact);
        assert_eq!(
            orders.records[0].quantity.normalized.as_deref(),
            Some("100")
        );

        let execution_columns =
            columns(&["证券代码", "买卖方向", "成交价格", "成交数量", "成交金额"]);
        let execution_rows = vec![TableRow {
            ordinal: 0,
            cells: vec![
                exact("600028 中国石化"),
                exact("卖出"),
                exact("5.0500"),
                exact("100"),
                exact("505.00"),
            ],
        }];
        let (executions, execution_quality) = execution_table(&execution_columns, &execution_rows);
        assert_eq!(execution_quality, DataQuality::Exact);
        assert_eq!(
            executions.records[0].amount.normalized.as_deref(),
            Some("505.00")
        );

        let funds_columns = columns(&["总资产", "可用资金", "冻结资金"]);
        let funds_rows = vec![TableRow {
            ordinal: 0,
            cells: vec![exact("10,000.00"), exact("1,000.00"), exact("0.00")],
        }];
        let (funds, funds_quality) = funds_table(&funds_columns, &funds_rows);
        assert_eq!(funds_quality, DataQuality::Exact);
        assert_eq!(
            funds.records[0].available_funds.normalized.as_deref(),
            Some("1000.00")
        );
    }

    #[test]
    fn maps_real_order_header_aliases() {
        let order_columns = columns(&[
            "证券代码",
            "证券名称",
            "申请编号",
            "买卖标志",
            "委托状态",
            "委托价格",
            "委托数量",
            "撤单标志",
        ]);
        let order_rows = vec![TableRow {
            ordinal: 0,
            cells: vec![
                exact("600028"),
                exact("中国石化"),
                exact("42"),
                exact("买入"),
                exact("已报"),
                exact("5.0500"),
                exact("100"),
                exact("可撤"),
            ],
        }];

        let (orders, quality) = order_table(&order_columns, &order_rows);
        let record = &orders.records[0];
        assert_eq!(record.order_id.normalized.as_deref(), Some("42"));
        assert_eq!(record.side.normalized.as_deref(), Some("买入"));
        assert_eq!(record.cancelable.normalized.as_deref(), Some("可撤"));
        assert_eq!(quality, DataQuality::Exact);
    }

    #[test]
    fn maps_real_execution_header_aliases() {
        let execution_columns = columns(&[
            "证券代码",
            "证券名称",
            "成交时间",
            "申请编号",
            "买卖标志",
            "成交价格",
            "成交数量",
            "成交金额",
            "成交编号",
        ]);
        let execution_rows = vec![TableRow {
            ordinal: 0,
            cells: vec![
                exact("600028"),
                exact("中国石化"),
                exact("09:31:00"),
                exact("42"),
                exact("买入"),
                exact("5.0500"),
                exact("100"),
                exact("505.00"),
                exact("84"),
            ],
        }];

        let (executions, quality) = execution_table(&execution_columns, &execution_rows);
        let record = &executions.records[0];
        assert_eq!(record.order_id.normalized.as_deref(), Some("42"));
        assert_eq!(record.side.normalized.as_deref(), Some("买入"));
        assert_eq!(record.execution_id.normalized.as_deref(), Some("84"));
        assert_eq!(quality, DataQuality::Exact);
    }

    #[test]
    fn preserves_distinct_executions_with_identical_trade_values() {
        let execution_columns = columns(&[
            "证券代码",
            "成交时间",
            "成交编号",
            "买卖标志",
            "成交价格",
            "成交数量",
            "成交金额",
        ]);
        let execution_rows = ["84", "85"]
            .into_iter()
            .enumerate()
            .map(|(ordinal, execution_id)| TableRow {
                ordinal,
                cells: vec![
                    exact("600028 中国石化"),
                    exact("09:31:00"),
                    exact(execution_id),
                    exact("买入"),
                    exact("5.05"),
                    exact("100"),
                    exact("505.00"),
                ],
            })
            .collect::<Vec<_>>();

        let (executions, quality) = execution_table(&execution_columns, &execution_rows);
        assert_eq!(executions.records.len(), 2);
        assert_eq!(quality, DataQuality::Exact);
    }

    #[test]
    fn recovers_only_consistent_merged_execution_values() {
        let execution_columns =
            columns(&["证券代码", "买卖标志", "成交价格", "成交数量", "成交金额"]);
        let consistent = vec![TableRow {
            ordinal: 0,
            cells: vec![
                exact("600028 中国石化"),
                exact("买入"),
                exact("5.05"),
                unavailable(),
                partial("100 505.00"),
            ],
        }];
        let (executions, quality) = execution_table(&execution_columns, &consistent);
        assert_eq!(
            executions.records[0].quantity.normalized.as_deref(),
            Some("100")
        );
        assert_eq!(
            executions.records[0].amount.normalized.as_deref(),
            Some("505.00")
        );
        assert_eq!(quality, DataQuality::Exact);

        let inconsistent = vec![TableRow {
            ordinal: 0,
            cells: vec![
                exact("600028 中国石化"),
                exact("买入"),
                exact("5.05"),
                unavailable(),
                partial("100 900.00"),
            ],
        }];
        let (executions, quality) = execution_table(&execution_columns, &inconsistent);
        assert_eq!(executions.records[0].quantity.normalized, None);
        assert_eq!(quality, DataQuality::Partial);
    }

    #[test]
    fn recovers_funds_values_merged_across_adjacent_columns() {
        let funds_columns = columns(&[
            "资金余额",
            "可用资金",
            "可取资金",
            "冻结金额",
            "证券市值",
            "现金资产",
            "总资产",
        ]);
        let funds_rows = vec![TableRow {
            ordinal: 0,
            cells: vec![
                exact("19.91"),
                exact("19.91"),
                exact("19.91"),
                unavailable(),
                partial("0 28090.00"),
                unavailable(),
                partial("19.91 28109.910"),
            ],
        }];

        let (funds, quality) = funds_table(&funds_columns, &funds_rows);
        let record = &funds.records[0];
        assert_eq!(record.frozen_funds.normalized.as_deref(), Some("0"));
        assert_eq!(record.market_value.normalized.as_deref(), Some("28090.00"));
        assert_eq!(record.cash_balance.normalized.as_deref(), Some("19.91"));
        assert_eq!(record.total_assets.normalized.as_deref(), Some("28109.910"));
        assert_eq!(record.total_assets.source, DataSource::Inferred);
        assert_eq!(quality, DataQuality::Exact);
    }

    #[test]
    fn recovers_numeric_pair_when_other_cell_has_placeholder() {
        let funds_columns = columns(&["冻结金额", "证券市值"]);
        let funds_rows = vec![TableRow {
            ordinal: 0,
            cells: vec![partial("-"), partial("0 28090.00")],
        }];

        let (funds, _) = funds_table(&funds_columns, &funds_rows);
        let record = &funds.records[0];
        assert_eq!(record.frozen_funds.normalized.as_deref(), Some("0"));
        assert_eq!(record.market_value.normalized.as_deref(), Some("28090.00"));
    }

    #[test]
    fn prefers_dedicated_cash_balance_over_inferred_cash_assets() {
        let funds_columns = columns(&["资金余额", "现金资产", "总资产"]);
        let funds_rows = vec![TableRow {
            ordinal: 0,
            cells: vec![exact("19.91"), unavailable(), partial("19.91 28109.910")],
        }];

        let (funds, _) = funds_table(&funds_columns, &funds_rows);
        assert_eq!(
            funds.records[0].cash_balance.normalized.as_deref(),
            Some("19.91")
        );
        assert_eq!(funds.records[0].cash_balance.source, DataSource::Ocr);
    }

    fn position_columns() -> Vec<TableColumn> {
        columns(&["证券代码", "证券名称", "可用股份", "当前价"])
    }

    fn columns(titles: &[&str]) -> Vec<TableColumn> {
        titles
            .into_iter()
            .enumerate()
            .map(|(ordinal, title)| TableColumn {
                ordinal,
                identifier: None,
                title: exact(*title),
                position: None,
                size: None,
            })
            .collect()
    }

    fn exact(value: &str) -> ObservedText {
        ObservedText {
            value: Some(value.to_string()),
            quality: DataQuality::Exact,
            source: DataSource::Ocr,
        }
    }

    fn partial(value: &str) -> ObservedText {
        ObservedText {
            value: Some(value.to_string()),
            quality: DataQuality::Partial,
            source: DataSource::Ocr,
        }
    }

    fn unavailable() -> ObservedText {
        ObservedText {
            value: None,
            quality: DataQuality::Unavailable,
            source: DataSource::Unavailable,
        }
    }
}

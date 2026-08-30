use clap::{Args, Subcommand, ValueEnum};

use crate::pages::{
    CancellationTarget, OrderSide, ReferenceQuote, StageOrderRequest, TradePreflightRequest,
};

#[derive(Debug, clap::Parser)]
#[command(name = "stock-operator", about = "Local macOS stock UI operator")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Read-only diagnostics and current UI state.
    #[command(subcommand)]
    Inspect(InspectCommand),
    /// Read account and trading tables.
    #[command(subcommand)]
    Read(ReadCommand),
    /// Navigate allowlisted trading panels.
    #[command(subcommand)]
    Navigate(NavigateCommand),
    /// Select securities and stage or supervise an order.
    #[command(subcommand)]
    Order(OrderCommand),
    /// Supervise a single-order cancellation.
    #[command(subcommand)]
    Cancel(CancelCommand),
}

#[derive(Debug, Subcommand)]
pub(crate) enum InspectCommand {
    /// Print the REST OpenAPI 3.1 document.
    Openapi,
    /// Print a bounded Accessibility tree.
    Probe(SnapshotArgs),
    /// Identify the current workspace and panel.
    View,
    /// List semantic controls in the current window.
    Inventory {
        #[arg(long, default_value_t = 200)]
        limit: usize,
    },
    /// Read the current buy or sell form.
    Form,
    /// Run Vision OCR over the current target window.
    Ocr,
    /// Diagnose the Accessibility positions table.
    PositionsTable,
    /// Read broker-side trade facts and optionally validate an intended order.
    Preflight(PreflightArgs),
}

#[derive(Debug, Args)]
pub(crate) struct SnapshotArgs {
    #[arg(long)]
    pub(crate) max_depth: Option<usize>,
    #[arg(long)]
    pub(crate) max_nodes: Option<usize>,
}

#[derive(Debug, Args)]
pub(crate) struct PreflightArgs {
    #[arg(long)]
    pub(crate) security_code: Option<String>,
    #[arg(long)]
    pub(crate) side: Option<CliOrderSide>,
    #[arg(long)]
    pub(crate) price: Option<String>,
    #[arg(long)]
    pub(crate) quantity: Option<u64>,
    #[arg(long)]
    pub(crate) reference_price: Option<String>,
    #[arg(long, default_value = "backend")]
    pub(crate) reference_source: String,
    #[arg(long)]
    pub(crate) reference_captured_at: Option<String>,
}

impl PreflightArgs {
    pub(crate) fn into_request(self) -> anyhow::Result<TradePreflightRequest> {
        let order_fields = [
            self.security_code.is_some(),
            self.side.is_some(),
            self.price.is_some(),
            self.quantity.is_some(),
        ];
        let order = if order_fields.iter().all(|value| !value) {
            None
        } else if order_fields.iter().all(|value| *value) {
            Some(StageOrderRequest {
                security_code: self.security_code.unwrap(),
                side: self.side.unwrap().into(),
                price: self.price.unwrap(),
                quantity: self.quantity.unwrap(),
            })
        } else {
            anyhow::bail!("security-code, side, price, and quantity must be provided together")
        };
        let reference_quote = self.reference_price.map(|latest_price| ReferenceQuote {
            latest_price,
            source: self.reference_source,
            captured_at: self.reference_captured_at,
        });
        Ok(TradePreflightRequest {
            order,
            reference_quote,
        })
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum ReadCommand {
    /// Read positions; structured output is the default.
    Positions {
        #[arg(long)]
        source: Option<ReadSource>,
    },
    /// Read same-day orders; structured output is the default.
    Orders {
        #[arg(long)]
        source: Option<ReadSource>,
    },
    /// Read cancellation candidates; OCR output is the default.
    Cancellations {
        #[arg(long)]
        source: Option<ReadSource>,
    },
    /// Read same-day executions; structured output is the default.
    Executions {
        #[arg(long)]
        source: Option<ReadSource>,
    },
    /// Read funds; structured output is the default.
    Funds {
        #[arg(long)]
        source: Option<ReadSource>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum ReadSource {
    Ax,
    Ocr,
    Structured,
}

#[derive(Debug, Subcommand)]
pub(crate) enum NavigateCommand {
    /// Open positions.
    Positions,
    /// Open same-day orders.
    Orders,
    /// Open same-day executions.
    Executions,
    /// Open funds.
    Funds,
    /// Open the cancellation business page.
    Cancellations,
    /// List currently exposed AX navigation candidates.
    Candidates,
}

#[derive(Debug, Subcommand)]
pub(crate) enum OrderCommand {
    /// Select and verify a security without staging values.
    SelectSecurity { security_code: String },
    /// Select a security and stage price and quantity without submitting.
    Stage {
        #[command(flatten)]
        request: OrderArgs,
    },
    /// Open the broker confirmation dialog for a supervised live order.
    OpenConfirmation {
        #[command(flatten)]
        request: OrderArgs,
        #[arg(long)]
        live: bool,
    },
    /// Confirm an already open, exactly matching broker dialog.
    Confirm {
        #[command(flatten)]
        request: OrderArgs,
        #[arg(long)]
        live: bool,
    },
    /// Close an exactly matching successful submission notice.
    CloseNotice {
        contract_id: String,
        #[arg(long)]
        live: bool,
    },
}

#[derive(Debug, Args)]
pub(crate) struct OrderArgs {
    #[arg(long)]
    pub(crate) security_code: String,
    #[arg(long)]
    pub(crate) side: CliOrderSide,
    #[arg(long)]
    pub(crate) price: String,
    #[arg(long)]
    pub(crate) quantity: u64,
}

impl From<OrderArgs> for StageOrderRequest {
    fn from(value: OrderArgs) -> Self {
        Self {
            security_code: value.security_code,
            side: value.side.into(),
            price: value.price,
            quantity: value.quantity,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum CliOrderSide {
    Buy,
    Sell,
}

impl From<CliOrderSide> for OrderSide {
    fn from(value: CliOrderSide) -> Self {
        match value {
            CliOrderSide::Buy => Self::Buy,
            CliOrderSide::Sell => Self::Sell,
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum CancelCommand {
    /// Double-click one verified contract and open its cancellation dialog.
    OpenConfirmation {
        #[command(flatten)]
        target: CancellationArgs,
        #[arg(long)]
        live: bool,
    },
    /// Confirm an already open, exactly matching cancellation dialog.
    Confirm {
        #[command(flatten)]
        target: CancellationArgs,
        #[arg(long)]
        live: bool,
    },
    /// Close an exact "cancellation submitted" notice.
    CloseNotice {
        #[arg(long)]
        live: bool,
    },
    /// Dismiss only the exact missing-selection warning.
    DismissSelectionWarning {
        #[arg(long)]
        live: bool,
    },
}

#[derive(Debug, Args)]
pub(crate) struct CancellationArgs {
    #[arg(long)]
    pub(crate) contract_id: String,
    #[arg(long)]
    pub(crate) security_code: String,
    #[arg(long)]
    pub(crate) security_name: String,
    #[arg(long)]
    pub(crate) side: String,
    #[arg(long)]
    pub(crate) price: String,
    #[arg(long)]
    pub(crate) quantity: u64,
}

impl From<CancellationArgs> for CancellationTarget {
    fn from(value: CancellationArgs) -> Self {
        Self {
            contract_id: value.contract_id,
            security_code: value.security_code,
            security_name: value.security_name,
            side: value.side,
            price: value.price,
            quantity: value.quantity,
        }
    }
}

#[cfg(test)]
mod tests;

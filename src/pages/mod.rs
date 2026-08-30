mod business;
mod cancel_order;
mod helpers;
mod mouse;
mod navigation;
mod ocr;
pub(crate) mod ocr_navigation;
mod ocr_table;
mod panel_ocr;
mod panel_reader;
mod positions_ocr;
mod reader;
mod security_selection;
mod stage_order;
mod stage_order_validation;
mod submit_order;
mod trade_preflight;
#[cfg(test)]
mod trade_preflight_tests;
mod trade_preflight_validation;
mod types;

pub use business::{
    ExecutionStructuredSnapshot, FundsStructuredSnapshot, OrderStructuredSnapshot,
    PositionStructuredSnapshot,
};
pub use cancel_order::CancellationTarget;
pub use navigation::{NavigationCandidates, NavigationResult, NavigationTarget};
pub use ocr::OcrSnapshot;
pub use reader::PageReader;
pub use stage_order::{OrderSide, StageOrderRequest, StageOrderResult};
pub use trade_preflight::{ReferenceQuote, TradePreflightRequest, TradePreflightResult};
pub use types::{
    ExecutionsSnapshot, FundsSnapshot, OrdersSnapshot, PageSnapshot, PositionsSnapshot,
    PositionsTableDiagnostic, TradeFormSnapshot, UiInventory, ViewDescriptor,
};

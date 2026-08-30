use anyhow::{Result, bail};
use clap::Parser;
use tracing::info;

use crate::{
    ax::AccessibilityInspector,
    cli::{self, Cli, Command, InspectCommand, NavigateCommand, OrderCommand, ReadCommand},
    config::OperatorConfig,
    mcp::serve,
    operator_service::{
        OperatorService, ReadPanel, ReadRepresentation, ReadRequest, SelectSecurityRequest,
    },
    pages::NavigationTarget,
};

pub async fn run() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    let config = OperatorConfig::from_env()?;
    let inspector = AccessibilityInspector::new(config.clone());
    let service = OperatorService::new(inspector.clone());
    if !inspector.request_permission_prompt() {
        info!("Accessibility permission is not granted yet");
    }
    if let Some(command) = cli.command {
        return dispatch(command, &config, &service).await;
    }
    info!(bind_addr = %config.bind_addr, endpoint = %config.endpoint(), "starting stock operator HTTP and MCP server");
    serve(config, inspector, service).await
}

async fn dispatch(
    command: Command,
    config: &OperatorConfig,
    service: &OperatorService,
) -> Result<()> {
    match command {
        Command::Inspect(command) => inspect(command, config, service).await,
        Command::Read(command) => read(command, service).await,
        Command::Navigate(command) => navigate(command, service).await,
        Command::Order(command) => order(command, service).await,
        Command::Cancel(command) => cancel(command, service).await,
    }
}

async fn inspect(
    command: InspectCommand,
    config: &OperatorConfig,
    service: &OperatorService,
) -> Result<()> {
    let value = match command {
        InspectCommand::Openapi => serde_json::to_value(crate::http_api::openapi_document())?,
        InspectCommand::Probe(args) => serde_json::to_value(
            service
                .snapshot(
                    args.max_depth.unwrap_or(config.max_depth).clamp(1, 12),
                    args.max_nodes.unwrap_or(config.max_nodes).clamp(1, 2_000),
                )
                .await?,
        )?,
        InspectCommand::View => serde_json::to_value(service.view().await?)?,
        InspectCommand::Inventory { limit } => service.inventory(limit).await?,
        InspectCommand::Form => service.trade_form().await?,
        InspectCommand::Ocr => service.ocr_visible_text().await?,
        InspectCommand::PositionsTable => service.positions_diagnostic().await?,
        InspectCommand::Preflight(args) => {
            serde_json::to_value(service.trade_preflight(args.into_request()?).await?)?
        }
    };
    print_json(value)
}

async fn read(command: ReadCommand, service: &OperatorService) -> Result<()> {
    let (panel, representation) = match command {
        ReadCommand::Positions { source } => (ReadPanel::Positions, source),
        ReadCommand::Orders { source } => (ReadPanel::Orders, source),
        ReadCommand::Executions { source } => (ReadPanel::Executions, source),
        ReadCommand::Funds { source } => (ReadPanel::Funds, source),
        ReadCommand::Cancellations { source } => (ReadPanel::Cancellations, source),
    };
    print_json(
        service
            .read(ReadRequest {
                panel,
                representation: representation.map(read_representation),
            })
            .await?,
    )
}

async fn navigate(command: NavigateCommand, service: &OperatorService) -> Result<()> {
    let value = match command {
        NavigateCommand::Positions => {
            serde_json::to_value(service.navigate(NavigationTarget::Positions).await?)?
        }
        NavigateCommand::Orders => {
            serde_json::to_value(service.navigate(NavigationTarget::Orders).await?)?
        }
        NavigateCommand::Executions => {
            serde_json::to_value(service.navigate(NavigationTarget::Executions).await?)?
        }
        NavigateCommand::Funds => {
            serde_json::to_value(service.navigate(NavigationTarget::Funds).await?)?
        }
        NavigateCommand::Cancellations => service.navigate_cancellations().await?,
        NavigateCommand::Candidates => service.navigation_candidates().await?,
    };
    print_json(value)
}

async fn order(command: OrderCommand, service: &OperatorService) -> Result<()> {
    match command {
        OrderCommand::SelectSecurity { security_code } => print_json(
            service
                .select_security(SelectSecurityRequest { security_code })
                .await?,
        ),
        OrderCommand::Stage { request } => print_json(serde_json::to_value(
            service.stage_order(request.into()).await?,
        )?),
        OrderCommand::OpenConfirmation { request, live } => {
            require_live(live)?;
            print_json(
                service
                    .open_order_confirmation_direct(request.into())
                    .await?,
            )
        }
        OrderCommand::Confirm { request, live } => {
            require_live(live)?;
            print_json(service.confirm_order_direct(request.into()).await?)
        }
        OrderCommand::CloseNotice { contract_id, live } => {
            require_live(live)?;
            service.close_submitted_notice(&contract_id).await
        }
    }
}

async fn cancel(command: cli::CancelCommand, service: &OperatorService) -> Result<()> {
    match command {
        cli::CancelCommand::OpenConfirmation { target, live } => {
            require_live(live)?;
            let target = target.into();
            service.open_cancellation_direct(&target).await
        }
        cli::CancelCommand::Confirm { target, live } => {
            require_live(live)?;
            let target = target.into();
            service.confirm_cancellation_direct(&target).await
        }
        cli::CancelCommand::CloseNotice { live } => {
            require_live(live)?;
            service.close_cancel_submitted_notice().await
        }
        cli::CancelCommand::DismissSelectionWarning { live } => {
            require_live(live)?;
            service.close_cancel_selection_warning().await
        }
    }
}

fn read_representation(source: cli::ReadSource) -> ReadRepresentation {
    match source {
        cli::ReadSource::Ax => ReadRepresentation::Ax,
        cli::ReadSource::Ocr => ReadRepresentation::Ocr,
        cli::ReadSource::Structured => ReadRepresentation::Structured,
    }
}

fn require_live(live: bool) -> Result<()> {
    if !live {
        bail!("this command can mutate the broker UI; add --live explicitly")
    }
    Ok(())
}

fn print_json(value: serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

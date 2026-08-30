use clap::Parser;

use super::{CancelCommand, Cli, Command, InspectCommand, OrderCommand, ReadCommand, ReadSource};

#[test]
fn no_subcommand_preserves_server_mode() {
    assert!(
        Cli::try_parse_from(["stock-operator"])
            .unwrap()
            .command
            .is_none()
    );
}

#[test]
fn parses_structured_read_command() {
    let cli = Cli::try_parse_from([
        "stock-operator",
        "read",
        "executions",
        "--source",
        "structured",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Read(ReadCommand::Executions {
            source: Some(ReadSource::Structured)
        }))
    ));
}

#[test]
fn parses_preflight_order_and_reference_quote() {
    let cli = Cli::try_parse_from([
        "stock-operator",
        "inspect",
        "preflight",
        "--security-code",
        "600028",
        "--side",
        "buy",
        "--price",
        "5.06",
        "--quantity",
        "100",
        "--reference-price",
        "5.06",
        "--reference-source",
        "tushare",
    ])
    .unwrap();
    let Some(Command::Inspect(InspectCommand::Preflight(args))) = cli.command else {
        panic!("expected inspect preflight command");
    };
    let request = args.into_request().unwrap();
    assert_eq!(request.order.unwrap().security_code, "600028");
    assert_eq!(request.reference_quote.unwrap().source, "tushare");
}

#[test]
fn parses_typed_order_stage() {
    let cli = Cli::try_parse_from([
        "stock-operator",
        "order",
        "stage",
        "--security-code",
        "600028",
        "--side",
        "buy",
        "--price",
        "4.99",
        "--quantity",
        "100",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Order(OrderCommand::Stage { .. }))
    ));
}

#[test]
fn rejects_legacy_top_level_flags() {
    assert!(Cli::try_parse_from(["stock-operator", "--read-positions"]).is_err());
}

#[test]
fn live_confirmation_requires_explicit_flag_at_dispatch() {
    let args = [
        "stock-operator",
        "cancel",
        "confirm",
        "--contract-id",
        "3506784",
        "--security-code",
        "600028",
        "--security-name",
        "中国石化",
        "--side",
        "买入",
        "--price",
        "4.55",
        "--quantity",
        "100",
    ];
    let cli = Cli::try_parse_from(args).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Cancel(CancelCommand::Confirm { live: false, .. }))
    ));

    let mut live_args = args.to_vec();
    live_args.push("--live");
    let cli = Cli::try_parse_from(live_args).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Cancel(CancelCommand::Confirm { live: true, .. }))
    ));
}

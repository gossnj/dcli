use super::{Opt, Stat};
use clap::{error::ErrorKind, Parser};

#[test]
fn at_least_one_stat_is_required() {
    let error =
        Opt::try_parse_from(["dclistat", "--name", "One#1234"]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
}

#[test]
fn a_single_stat_is_accepted() {
    let opt = Opt::try_parse_from([
        "dclistat", "--name", "One#1234", "--stat", "kills",
    ])
    .unwrap();
    assert_eq!(opt.stat, [Stat::Kills]);
}

#[test]
fn space_separated_stats_preserve_requested_order() {
    let opt = Opt::try_parse_from([
        "dclistat", "--name", "One#1234", "--stat", "kills", "deaths", "kd",
    ])
    .unwrap();
    assert_eq!(opt.stat, [Stat::Kills, Stat::Deaths, Stat::KD]);
}

#[test]
fn repeated_stat_options_preserve_requested_order() {
    let opt = Opt::try_parse_from([
        "dclistat", "--name", "One#1234", "--stat", "kills", "--stat", "deaths",
    ])
    .unwrap();
    assert_eq!(opt.stat, [Stat::Kills, Stat::Deaths]);
}

#[test]
fn custom_moments_require_their_corresponding_dates() {
    for moment in ["--moment", "--end-moment"] {
        let error = Opt::try_parse_from([
            "dclistat", "--name", "One#1234", "--stat", "kills", moment,
            "custom",
        ])
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }
}

#[test]
fn custom_start_and_end_dates_are_accepted_together() {
    let opt = Opt::try_parse_from([
        "dclistat",
        "--name",
        "One#1234",
        "--stat",
        "kills",
        "--moment",
        "custom",
        "--custom-time",
        "2024-01-01T00:00:00Z",
        "--end-moment",
        "custom",
        "--end-custom-time",
        "2024-02-01T00:00:00Z",
    ])
    .unwrap();
    assert_eq!(opt.custom_time.unwrap().timestamp(), 1704067200);
    assert_eq!(opt.end_custom_time.unwrap().timestamp(), 1706745600);
}

#[test]
fn unknown_stats_are_rejected() {
    let error = Opt::try_parse_from([
        "dclistat",
        "--name",
        "One#1234",
        "--stat",
        "unknown_stat",
    ])
    .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::ValueValidation);
}

#[test]
fn player_name_remains_required() {
    let error =
        Opt::try_parse_from(["dclistat", "--stat", "kills"]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
}

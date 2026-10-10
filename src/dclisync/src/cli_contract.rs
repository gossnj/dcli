use super::Opt;
use clap::{error::ErrorKind, CommandFactory, FromArgMatches};

fn parse(args: &[&str]) -> Result<Opt, clap::Error> {
    // Isolate argument validation from the caller's optional API-key environment.
    let matches = Opt::command()
        .mut_arg("api_key", |arg| arg.env(None::<&str>))
        .try_get_matches_from(args)?;
    Opt::from_arg_matches(&matches)
}

#[test]
fn bare_sync_selects_all_players() {
    let opt = parse(&["dclisync", "--sync", "--api-key", "test-key"]).unwrap();
    assert!(opt.sync.unwrap().is_empty());
}

#[test]
fn named_sync_preserves_multiple_players() {
    let opt = parse(&[
        "dclisync",
        "--sync",
        "Player One#1234",
        "Two#5678",
        "--api-key",
        "test-key",
    ])
    .unwrap();
    let names: Vec<_> = opt
        .sync
        .unwrap()
        .iter()
        .map(|p| p.get_bungie_name())
        .collect();
    assert_eq!(names, ["Player One#1234", "Two#5678"]);
}

#[test]
fn add_accepts_multiple_names_and_stops_at_the_next_option() {
    let opt = parse(&[
        "dclisync",
        "--add",
        "One#1234",
        "Two#5678",
        "--list",
        "--api-key",
        "test-key",
    ])
    .unwrap();
    let names: Vec<_> = opt
        .add
        .unwrap()
        .iter()
        .map(|p| p.get_bungie_name())
        .collect();
    assert_eq!(names, ["One#1234", "Two#5678"]);
    assert!(opt.list);
    assert!(opt.sync.is_none());
}

#[test]
fn remove_accepts_multiple_names_without_an_api_key() {
    let opt = parse(&["dclisync", "--remove", "One#1234", "Two#5678"]).unwrap();
    let names: Vec<_> = opt
        .remove
        .unwrap()
        .iter()
        .map(|p| p.get_bungie_name())
        .collect();
    assert_eq!(names, ["One#1234", "Two#5678"]);
}

#[test]
fn add_and_remove_require_at_least_one_name() {
    for action in ["--add", "--remove"] {
        let err =
            parse(&["dclisync", action, "--api-key", "test-key"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidValue);
    }
}

#[test]
fn api_actions_require_a_key() {
    for args in [
        vec!["dclisync", "--sync", "One#1234"],
        vec!["dclisync", "--add", "One#1234"],
        vec!["dclisync", "--import-group", "123"],
    ] {
        assert_eq!(
            parse(&args).unwrap_err().kind(),
            ErrorKind::MissingRequiredArgument
        );
    }
}

#[test]
fn list_does_not_require_a_key_or_trigger_sync() {
    let opt = parse(&["dclisync", "--list"]).unwrap();
    assert!(opt.list);
    assert!(opt.sync.is_none());
    assert!(opt.api_key.is_none());
}

#[test]
fn an_action_is_required() {
    assert_eq!(
        parse(&["dclisync"]).unwrap_err().kind(),
        ErrorKind::MissingRequiredArgument
    );
}

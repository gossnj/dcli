use dcli::enums::mode::Mode;
use dcli::response::activities::ActivitiesResponse;
use serde_json::json;

#[test]
fn unknown_activity_modes_parse_history_and_preserve_numeric_ids() {
    for id in [94_u32, 123456, u32::MAX] {
        let mut values = serde_json::Map::new();
        for name in [
            "assists",
            "score",
            "kills",
            "deaths",
            "completed",
            "opponentsDefeated",
            "efficiency",
            "killsDeathsRatio",
            "killsDeathsAssists",
            "activityDurationSeconds",
            "completionReason",
            "startSeconds",
            "timePlayedSeconds",
            "playerCount",
            "teamScore",
            "fireteamId",
        ] {
            values.insert(name.into(), json!({"basic":{"value":1}}));
        }
        let payload = json!({"Response":{"activities":[{"period":"2026-01-01T12:00:00Z",
            "activityDetails":{"referenceId":1,"directorActivityHash":1,"instanceId":"1",
                "mode":id,"modes":[5,id],"isPrivate":false,"membershipType":3},"values":values}]},
            "ErrorCode":1,"ThrottleSeconds":0,"ErrorStatus":"Success","Message":"Ok","MessageData":{}});
        let parsed: ActivitiesResponse = serde_json::from_value(payload)
            .expect("valid unfamiliar mode must not reject the history page");
        let activity = parsed.response.unwrap().activities.unwrap().remove(0);
        assert_eq!(activity.details.mode.as_id(), id);
        assert_eq!(activity.details.modes[0], Mode::AllPvP);
        assert_eq!(activity.details.modes[1].as_id(), id);
        assert_eq!(
            serde_json::to_value(activity.details.mode).unwrap(),
            json!(id)
        );
        assert_eq!(Mode::from_id(id).unwrap().as_id(), id);
    }
}

#[test]
fn unknown_activity_modes_keep_known_ids_and_reject_malformed_values() {
    let known: Mode = serde_json::from_str("71").unwrap();
    assert_eq!(known, Mode::ClashQuickplay);
    assert_eq!(serde_json::to_string(&known).unwrap(), "71");
    for malformed in ["-1", "4294967296", "94.5", "\"94\"", "null"] {
        assert!(
            serde_json::from_str::<Mode>(malformed).is_err(),
            "accepted {}",
            malformed
        );
    }
}

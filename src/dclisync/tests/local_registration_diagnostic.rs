use dcli::activitystoreinterface::ActivityStoreInterface;
use dcli::crucible::PlayerName;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn seeded_member_registration_reports_its_native_error() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("dcli-registration-{}-{}", std::process::id(), nonce));
    fs::create_dir(&directory).unwrap();

    let mut store = ActivityStoreInterface::init_with_path(&directory, Some("offline-fixture".into()))
        .await
        .expect("initialize native activity store");
    let database = directory.join("dcli.sqlite3");
    let seeded = Command::new("sqlite3")
        .arg(&database)
        .arg("INSERT INTO member(member_id,platform_id,display_name,bungie_display_name,bungie_display_name_code) VALUES(10,3,'Fixture','Fixture','1234'),(20,3,'Other','Other','5678'); INSERT INTO character(character_id,member,class) VALUES(100,10,1),(200,20,2);")
        .output()
        .expect("sqlite3 is available for this local diagnostic");
    assert!(seeded.status.success(), "seed fixture: {}", String::from_utf8_lossy(&seeded.stderr));

    let name = PlayerName::from_bungie_name("Fixture#1234");
    assert!(name.is_valid_bungie_name());
    for attempt in 1..=2 {
        if let Err(error) = store.add_player_to_sync(&name).await {
            panic!("registration attempt {} failed: {:?}", attempt, error);
        }
    }

    let triggered = Command::new("sqlite3")
        .arg(&database)
        .arg("CREATE TRIGGER reject_sync BEFORE INSERT ON sync BEGIN SELECT RAISE(ABORT,'fixture rejects registration'); END;")
        .output()
        .expect("install registration failure fixture");
    assert!(triggered.status.success(), "install trigger: {}", String::from_utf8_lossy(&triggered.stderr));
    let failure = store.add_player_to_sync(&name).await.unwrap_err();
    eprintln!("expected native registration failure: {:?}", failure);

    drop(store);
    fs::remove_dir_all(directory).unwrap();
}

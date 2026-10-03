use serde::Serialize;
use sqlx::Error as SqlxError;
use std::time::Instant;

#[derive(Serialize)]
pub struct InitializationEvent {
    pub role: &'static str,
    pub stage: &'static str,
    pub outcome: &'static str,
    pub elapsed_ms: u128,
    pub category: Option<&'static str>,
    pub sqlite_code: Option<i32>,
}

#[derive(Default)]
pub struct InitializationDiagnostics {
    pub schema_version_before: Option<i32>,
    pub events: Vec<InitializationEvent>,
}

impl InitializationDiagnostics {
    pub fn record(
        &mut self,
        role: &'static str,
        stage: &'static str,
        started: Instant,
        category: Option<&'static str>,
        sqlite_code: Option<i32>,
    ) {
        self.events.push(InitializationEvent {
            role,
            stage,
            outcome: if category.is_some() { "error" } else { "ok" },
            elapsed_ms: started.elapsed().as_millis(),
            category,
            sqlite_code,
        });
    }

    pub fn record_sqlx<T>(
        &mut self,
        role: &'static str,
        stage: &'static str,
        started: Instant,
        result: &Result<T, SqlxError>,
    ) {
        match result {
            Ok(_) => self.record(role, stage, started, None, None),
            Err(error) => {
                let (category, code) = match error {
                    SqlxError::Database(database) => (
                        "sqlite",
                        database
                            .code()
                            .and_then(|code| code.parse::<i32>().ok()),
                    ),
                    SqlxError::Io(_) => ("io", None),
                    _ => ("other", None),
                };
                self.record(role, stage, started, Some(category), code);
            }
        }
    }
}

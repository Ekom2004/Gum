use gum_store::pg::PostgresStore;
use gum_store::models::ProjectRecord;

#[derive(Clone)]
pub struct AppState {
    pub store: PostgresStore,
    pub project_id: String,
}

impl AppState {
    pub fn for_dev() -> Result<Self, String> {
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(value) => value,
            Err(_) => "postgresql://ekomotu@127.0.0.1:5432/gum_dev".to_string(),
        };
        let store = PostgresStore::connect(&database_url)?;
        let project = ProjectRecord {
            id: "proj_dev".to_string(),
            name: "Gum Dev".to_string(),
            slug: "gum-dev".to_string(),
            api_key_hash: "dev".to_string(),
        };
        store.prepare_dev_database(&project)?;

        Ok(Self {
            store,
            project_id: "proj_dev".to_string(),
        })
    }
}

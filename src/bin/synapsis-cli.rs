use synapsis::infrastructure::database::sqlite::Database;
use synapsis_core::domain::entities::{Observation, SearchParams};
use synapsis_core::domain::ports::StoragePort;
use synapsis_core::domain::types::{ObservationType, SessionId};

fn main() {
    let db = Database::new();
    db.init().expect("Failed to initialize database");

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: synapsis-cli <command> [args]");
        eprintln!("Commands: save, recall, search");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "save" => {
            let content = args[2..].join(" ");
            let obs = Observation::new(
                SessionId::new("cli"),
                ObservationType::Manual,
                "memory".to_string(),
                content,
            );
            match db.save_observation(&obs) {
                Ok(id) => println!("Saved observation: {}", id.0),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        "recall" => {
            let limit: usize = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(20);
            match db.recent_observations(limit) {
                Ok(obs) => {
                    for o in obs {
                        println!("[{}] {}: {}", o.id.0, o.created_at, o.content);
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        "search" => {
            let query = args[2..].join(" ");
            let params = SearchParams::new(query);
            match db.search_observations(&params) {
                Ok(results) => {
                    for r in results {
                        println!("[{}] {} (score: {:.2})", r.id.0, r.content, r.score);
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            std::process::exit(1);
        }
    }
}

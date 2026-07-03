use dotenv::dotenv;
use sqids::Sqids;
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

fn header() {
    info!(r#"▄▄▄▄▄   ▄▄▄▄▄▄▄     ▄▄▄▄    ▄▄▄▄▄▄▄ ▄▄▄   ▄▄▄  ▄▄▄▄▄▄▄ ▄▄▄▄▄▄▄   "#);
    info!(r#"██▀▀██▄ ███▀▀███▄ ▄██▀▀██▄ ███▀▀▀▀▀ ███ ▄███▀ ███▀▀▀▀▀ ███▀▀███▄ "#);
    info!(r#"██  ███ ███▄▄███▀ ███  ███ ███      ███████   ███▄▄    ███▄▄███▀ "#);
    info!(r#"██  ███ ███▀▀██▄  ███▀▀███ ███      ███▀███▄  ███      ███▀▀██▄  "#);
    info!(r#"█████▀  ███  ▀███ ███  ███ ▀███████ ███  ▀███ ▀███████ ███  ▀███ "#);
}

fn tracing() {
    let level = match cfg!(debug_assertions) {
        true => Level::DEBUG,
        false => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder().with_max_level(level).finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}

pub fn sqids() -> Result<Sqids, sqids::Error> {
    Sqids::builder().min_length(1).build()
}

pub fn init() {
    dotenv().ok();
    tracing();
    header();
}

#[cfg(test)]
mod tests {
    use super::sqids;

    #[test]
    fn sqids_roundtrips_ids() {
        let s = sqids().unwrap();
        for id in [1u64, 2, 42, 1000, u64::from(u32::MAX)] {
            let slug = s.encode(&[id]).unwrap();
            assert!(!slug.is_empty());
            assert_eq!(s.decode(&slug).first().copied(), Some(id));
        }
    }

    #[test]
    fn sqids_rejects_garbage_slug() {
        let s = sqids().unwrap();
        // a slug that decodes to nothing must not yield an id
        assert!(s.decode("!!!!").is_empty());
    }
}

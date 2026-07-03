use sea_orm::Order;
use std::str::FromStr;

pub const TAKE_DEF: u64 = 20;
pub const TAKE_MAX: u64 = 100;
pub const SKIP_DEF: u64 = 0;

pub fn skip(skip: Option<u64>, take: Option<u64>) -> (u64, u64) {
    let skip = skip.unwrap_or(SKIP_DEF);
    let mut take = take.unwrap_or(TAKE_DEF);

    if take > TAKE_MAX {
        take = TAKE_MAX;
    }

    (skip, take)
}

pub fn order(desc: Option<bool>, default: bool) -> Order {
    let is_desc = desc.unwrap_or(default);
    if is_desc { Order::Desc } else { Order::Asc }
}

pub fn column<C>(column: Option<String>, default: C) -> C
where
    C: FromStr,
{
    column.and_then(|s| s.parse::<C>().ok()).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_is_clamped_to_max_and_defaulted() {
        assert_eq!(skip(None, None), (SKIP_DEF, TAKE_DEF));
        assert_eq!(skip(Some(5), Some(10)), (5, 10));
        // over-max take is clamped so a client can't request the whole table
        assert_eq!(skip(Some(0), Some(10_000)), (0, TAKE_MAX));
    }

    #[test]
    fn order_defaults_and_overrides() {
        assert!(matches!(order(None, true), Order::Desc));
        assert!(matches!(order(None, false), Order::Asc));
        assert!(matches!(order(Some(false), true), Order::Asc));
        assert!(matches!(order(Some(true), false), Order::Desc));
    }

    #[test]
    fn column_falls_back_on_unparseable() {
        // u64 column: a garbage sort string yields the default, a valid one parses
        assert_eq!(column::<u64>(Some("nonsense".into()), 7), 7);
        assert_eq!(column::<u64>(Some("42".into()), 7), 42);
        assert_eq!(column::<u64>(None, 7), 7);
    }
}

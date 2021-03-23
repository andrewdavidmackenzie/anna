use crate::lattices::core_lattices::Lattice;
use std::mem::size_of_val;
use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

// TODO determine if u64 is enough resolution and/or compatible with cpp version - or we need u128
struct TimestampValuePair<T>
where
    T: Default,
{
    timestamp: u64,
    value: T,
}

impl<T> TimestampValuePair<T>
where
    T: Default,
{
    pub fn new(timestamp: u64, value: T) -> Self {
        TimestampValuePair { timestamp, value }
    }

    pub fn now(value: T) -> Result<Self, SystemTimeError> {
        let since_the_epoch = SystemTime::now().duration_since(UNIX_EPOCH)?;
        let now = since_the_epoch.as_secs();
        Ok(TimestampValuePair {
            timestamp: now,
            value,
        })
    }

    pub fn size(&self) -> usize {
        size_of_val(&self.value) + size_of_val(&self.timestamp)
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub fn value(&self) -> &T {
        &self.value
    }
}

impl<T> Default for TimestampValuePair<T>
where
    T: Default,
{
    fn default() -> Self {
        TimestampValuePair {
            timestamp: 0,
            value: T::default(),
        }
    }
}

struct LWWPairLattice<T>(TimestampValuePair<T>)
where
    T: Default;

impl<T> Lattice for LWWPairLattice<T>
where
    T: PartialOrd + PartialEq + Clone + Default,
{
    type A = T;

    fn do_merge(&mut self, l: &LWWPairLattice<T>) {
        if l.0.timestamp >= self.0.timestamp {
            self.0.timestamp = l.0.timestamp;
            self.0.value = l.0.value.clone();
        }
    }
}

impl<T> From<TimestampValuePair<T>> for LWWPairLattice<T>
where
    T: Default,
{
    fn from(tsv: TimestampValuePair<T>) -> Self {
        LWWPairLattice(tsv)
    }
}

impl<T> LWWPairLattice<T>
where
    T: PartialOrd + PartialEq + Clone + Default,
{
    pub fn size(&self) -> usize {
        self.0.size()
    }
}

#[cfg(test)]
mod test {
    use crate::lattices::core_lattices::Lattice;
    use crate::lattices::lww_pair_lattice::{LWWPairLattice, TimestampValuePair};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn new_time_value_pair_string() {
        let since_the_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let now = since_the_epoch.as_secs() as u64;
        let ts_v = TimestampValuePair::new(now, "Hello".to_string());
        assert_eq!(ts_v.value(), "Hello");
        assert_eq!(ts_v.timestamp(), now);
    }

    #[test]
    fn now_time_value_pair_string() {
        let since_the_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let now = since_the_epoch.as_secs() as u64;
        let ts_v = TimestampValuePair::now("Hello".to_string()).unwrap();
        assert_eq!(ts_v.value(), "Hello");
        assert!(ts_v.timestamp() >= now);
    }

    #[test]
    fn size() {
        let ts_v = TimestampValuePair::now(43u64).unwrap();
        assert_eq!(ts_v.size(), 8 + 8);
    }

    #[test]
    fn default_time_value_pair_u64() {
        let ts_v: TimestampValuePair<u64> = TimestampValuePair::default();
        assert_eq!(ts_v.value(), &0u64);
        assert_eq!(ts_v.timestamp(), 0);
    }

    #[test]
    fn merge_LWW() {
        let mut older: LWWPairLattice<_> =
            TimestampValuePair::new(123u64, "Older".to_string()).into();
        let newer: LWWPairLattice<_> = TimestampValuePair::new(456u64, "Newer".to_string()).into();
        older.do_merge(&newer);

        assert_eq!(older.0.value(), "Newer");
    }

    #[test]
    fn merge_LWW_newer() {
        let older: LWWPairLattice<_> = TimestampValuePair::new(123u64, "Older".to_string()).into();
        let mut newer: LWWPairLattice<_> =
            TimestampValuePair::new(456u64, "Newer".to_string()).into();
        newer.do_merge(&older);

        assert_eq!(newer.0.value(), "Newer");
    }
}

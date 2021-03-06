//! The `entry` module is a fundamental building block of Proof of History. It contains a
//! unique ID that is the hash of the Entry before it, plus the hash of the
//! transactions within it. Entries cannot be reordered, and its field `num_hashes`
//! represents an approximate amount of time since the last Entry was created.
use event::Event;
use hash::{extend_and_hash, hash, Hash};
use rayon::prelude::*;

/// Each Entry contains three pieces of data. The `num_hashes` field is the number
/// of hashes performed since the previous entry.  The `id` field is the result
/// of hashing `id` from the previous entry `num_hashes` times.  The `events`
/// field points to Events that took place shortly after `id` was generated.
///
/// If you divide `num_hashes` by the amount of time it takes to generate a new hash, you
/// get a duration estimate since the last Entry. Since processing power increases
/// over time, one should expect the duration `num_hashes` represents to decrease proportionally.
/// Though processing power varies across nodes, the network gives priority to the
/// fastest processor. Duration should therefore be estimated by assuming that the hash
/// was generated by the fastest processor at the time the entry was recorded.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Entry {
    pub num_hashes: u64,
    pub id: Hash,
    pub events: Vec<Event>,
}

impl Entry {
    /// Creates a Entry from the number of hashes `num_hashes` since the previous event
    /// and that resulting `id`.
    pub fn new_tick(num_hashes: u64, id: &Hash) -> Self {
        Entry {
            num_hashes,
            id: *id,
            events: vec![],
        }
    }

    /// Verifies self.id is the result of hashing a `start_hash` `self.num_hashes` times.
    /// If the event is not a Tick, then hash that as well.
    pub fn verify(&self, start_hash: &Hash) -> bool {
        self.events.par_iter().all(|event| event.verify())
            && self.id == next_hash(start_hash, self.num_hashes, &self.events)
    }
}

fn add_event_data(hash_data: &mut Vec<u8>, event: &Event) {
    match *event {
        Event::Transaction(ref tr) => {
            hash_data.push(0u8);
            hash_data.extend_from_slice(&tr.sig);
        }
        Event::Signature { ref sig, .. } => {
            hash_data.push(1u8);
            hash_data.extend_from_slice(sig);
        }
        Event::Timestamp { ref sig, .. } => {
            hash_data.push(2u8);
            hash_data.extend_from_slice(sig);
        }
    }
}

/// Creates the hash `num_hashes` after `start_hash`. If the event contains
/// signature, the final hash will be a hash of both the previous ID and
/// the signature.
pub fn next_hash(start_hash: &Hash, num_hashes: u64, events: &[Event]) -> Hash {
    let mut id = *start_hash;
    for _ in 1..num_hashes {
        id = hash(&id);
    }

    // Hash all the event data
    let mut hash_data = vec![];
    for event in events {
        add_event_data(&mut hash_data, event);
    }

    if !hash_data.is_empty() {
        return extend_and_hash(&id, &hash_data);
    }

    id
}

/// Creates the next Entry `num_hashes` after `start_hash`.
pub fn create_entry(start_hash: &Hash, cur_hashes: u64, events: Vec<Event>) -> Entry {
    let num_hashes = cur_hashes + if events.is_empty() { 0 } else { 1 };
    let id = next_hash(start_hash, 0, &events);
    Entry {
        num_hashes,
        id,
        events,
    }
}

/// Creates the next Tick Entry `num_hashes` after `start_hash`.
pub fn create_entry_mut(start_hash: &mut Hash, cur_hashes: &mut u64, events: Vec<Event>) -> Entry {
    let entry = create_entry(start_hash, *cur_hashes, events);
    *start_hash = entry.id;
    *cur_hashes = 0;
    entry
}

/// Creates the next Tick Entry `num_hashes` after `start_hash`.
pub fn next_tick(start_hash: &Hash, num_hashes: u64) -> Entry {
    Entry {
        num_hashes,
        id: next_hash(start_hash, num_hashes, &[]),
        events: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::prelude::*;
    use entry::create_entry;
    use event::Event;
    use hash::hash;
    use signature::{KeyPair, KeyPairUtil};
    use transaction::Transaction;

    #[test]
    fn test_entry_verify() {
        let zero = Hash::default();
        let one = hash(&zero);
        assert!(Entry::new_tick(0, &zero).verify(&zero)); // base case
        assert!(!Entry::new_tick(0, &zero).verify(&one)); // base case, bad
        assert!(next_tick(&zero, 1).verify(&zero)); // inductive step
        assert!(!next_tick(&zero, 1).verify(&one)); // inductive step, bad
    }

    #[test]
    fn test_event_reorder_attack() {
        let zero = Hash::default();

        // First, verify entries
        let keypair = KeyPair::new();
        let tr0 = Event::Transaction(Transaction::new(&keypair, keypair.pubkey(), 0, zero));
        let tr1 = Event::Transaction(Transaction::new(&keypair, keypair.pubkey(), 1, zero));
        let mut e0 = create_entry(&zero, 0, vec![tr0.clone(), tr1.clone()]);
        assert!(e0.verify(&zero));

        // Next, swap two events and ensure verification fails.
        e0.events[0] = tr1; // <-- attack
        e0.events[1] = tr0;
        assert!(!e0.verify(&zero));
    }

    #[test]
    fn test_witness_reorder_attack() {
        let zero = Hash::default();

        // First, verify entries
        let keypair = KeyPair::new();
        let tr0 = Event::new_timestamp(&keypair, Utc::now());
        let tr1 = Event::new_signature(&keypair, Default::default());
        let mut e0 = create_entry(&zero, 0, vec![tr0.clone(), tr1.clone()]);
        assert!(e0.verify(&zero));

        // Next, swap two witness events and ensure verification fails.
        e0.events[0] = tr1; // <-- attack
        e0.events[1] = tr0;
        assert!(!e0.verify(&zero));
    }

    #[test]
    fn test_next_tick() {
        let zero = Hash::default();
        assert_eq!(next_tick(&zero, 1).num_hashes, 1)
    }
}

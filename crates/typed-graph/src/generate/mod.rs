//! Generation module — GraphBuilder, random generation, scenarios, and adversarial strategies.
//!
//! This module provides the generation engines for constructing Gramps graphs.
//! The primary entry point is the [`GraphBuilder`] fluent API.

pub mod adversarial;
pub mod builder;
pub mod densify;
pub mod random;
pub use adversarial::*;
pub use builder::*;
pub use densify::*;
pub use random::*;

use rand::Rng;
use std::time::{SystemTime, UNIX_EPOCH};

/// Generate a Gramps-compatible handle: underscore + 16 hex characters.
///
/// Matches the format produced by Gramps' `create_id()`:
/// `_` + `%08x%08x` (timestamp_part, random_part).
///
/// The `u128 → u64 → u32` truncation chain on the timestamp is intentional:
/// it mirrors Gramps' `create_id()` which formats `time.time()*10000` (a
/// float whose integer part fits in ~45 bits) with `%08x`, taking the lower
/// 32 bits. This matches the same modulo-32-bit wrapping behavior.
pub fn generate_handle(rng: &mut impl Rng) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64; // u128 → u64 truncation
    let random: u32 = rng.gen();
    // u64 → u32 truncation via `as u32` matches Gramps' %08x wrapping
    format!("_{:08x}{:08x}", (ts / 10) as u32, random)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn test_generate_handle_format() {
        let mut rng = StdRng::seed_from_u64(42);
        let handle = generate_handle(&mut rng);
        // Should match: underscore + 16 hex chars
        assert!(handle.starts_with('_'), "Handle should start with underscore");
        assert_eq!(handle.len(), 17, "Handle should be 17 chars: _ + 16 hex");
        // The remaining 16 chars should all be hex digits
        let hex_part = &handle[1..];
        assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()), "Handle suffix should be hex digits");
    }

    #[test]
    fn test_generate_handle_has_underscore_prefix() {
        let mut rng = StdRng::seed_from_u64(99);
        let handle = generate_handle(&mut rng);
        assert!(handle.starts_with('_'));
    }

    #[test]
    fn test_generate_handle_unique() {
        let mut rng = StdRng::seed_from_u64(12345);
        let mut handles = std::collections::HashSet::new();
        for _ in 0..10_000 {
            let handle = generate_handle(&mut rng);
            assert!(handles.insert(handle), "Handle should be unique");
        }
    }
}

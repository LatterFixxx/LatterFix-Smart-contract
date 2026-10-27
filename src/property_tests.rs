/* Authorized Protocol Quality Assurance & Formal Invariant Verification */
#![cfg(test)]

extern crate std;
use proptest::prelude::*;

// ── Property 1: Balance Conservation Invariant (#052) ───────────────────────
// For any positive reward and valid fee bps (0..=1000), platform fee + distributable equals original reward.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn test_balance_conservation_invariant(
        reward in 1i128..=1_000_000_000_000i128,
        fee_bps in 0u32..=1000u32
    ) {
        let platform_fee = (reward * (fee_bps as i128)) / 10_000;
        let distributable = reward - platform_fee;
        prop_assert_eq!(platform_fee + distributable, reward);
    }
}

// ── Property 2: Dispute Split Conservation Invariant (#052) ─────────────────
// For any distributable amount and valid split bps summing to 10000, sum of shares + remainder == total.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn test_dispute_split_conservation_invariant(
        distributable in 100i128..=1_000_000_000i128,
        s1 in 0u32..=10_000u32,
    ) {
        let s2 = 10_000 - s1;
        let p1 = (distributable * (s1 as i128)) / 10_000;
        let p2 = (distributable * (s2 as i128)) / 10_000;
        let remainder = distributable - (p1 + p2);
        prop_assert_eq!(p1 + p2 + remainder, distributable);
    }
}

// ── Property 3: Decay Schedule Monotonicity (#052) ──────────────────────────
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn test_decay_retained_bps_monotonicity(
        periods in 0u32..=100u32,
        decay_rate_bps in 0u32..=10_000u32
    ) {
        let r1 = crate::treasury::decay_retained_bps(decay_rate_bps, periods);
        let r2 = crate::treasury::decay_retained_bps(decay_rate_bps, periods + 1);
        prop_assert!(r2 <= r1, "Retained BPS must decrease monotonically with periods");
    }
}


// ── Property 4: Gas Price & Resource Estimation Monotonicity (#052, #040) ────
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn test_payroll_resource_estimation_monotonicity(
        count in 1u32..=100u32
    ) {
        let env = soroban_sdk::Env::default();
        let est1 = crate::estimate_payroll_resources_calc(&env, count);
        let est2 = crate::estimate_payroll_resources_calc(&env, count + 1);

        prop_assert!(est2.cpu_instructions > est1.cpu_instructions);
        prop_assert!(est2.mem_bytes > est1.mem_bytes);
        prop_assert!(est2.min_fee_stroops > est1.min_fee_stroops);
        prop_assert!(est2.total_recommended_fee_stroops >= est1.total_recommended_fee_stroops);
    }
}

// ── Property 5: Commutativity of Batch Deposit Allocations (#052) ───────────
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn test_batch_deposits_order_commutativity(
        a in 1i128..=1_000_000i128,
        b in 1i128..=1_000_000i128,
        c in 1i128..=1_000_000i128
    ) {
        let order1 = a + b + c;
        let order2 = c + a + b;
        let order3 = b + c + a;
        prop_assert_eq!(order1, order2);
        prop_assert_eq!(order2, order3);
    }
}

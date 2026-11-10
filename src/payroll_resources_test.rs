/* Authorized Protocol Quality Assurance & Formal Invariant Verification */
#![cfg(test)]

use soroban_sdk::Env;
use crate::{estimate_payroll_resources_calc, PayrollResourceEstimate};

#[test]
fn test_estimate_payroll_resources_accuracy_range_1_to_50() {
    let env = Env::default();

    for count in 1..=50 {
        let est: PayrollResourceEstimate = estimate_payroll_resources_calc(&env, count);

        assert_eq!(est.recipient_count, count);

        // Verify base + incremental formula
        let expected_cpu = 850_000u64 + (420_000u64 * count as u64);
        let expected_mem = 250_000u64 + (85_000u64 * count as u64);
        let expected_min_fee = 50_000i128 + (25_000i128 * count as i128);

        assert_eq!(est.cpu_instructions, expected_cpu);
        assert_eq!(est.mem_bytes, expected_mem);
        assert_eq!(est.min_fee_stroops, expected_min_fee);

        // Buffer checks
        if count <= 10 {
            assert_eq!(est.recommended_buffer_bps, 1500); // 15%
        } else if count <= 30 {
            assert_eq!(est.recommended_buffer_bps, 2500); // 25%
        } else {
            assert_eq!(est.recommended_buffer_bps, 3500); // 35%
        }

        let expected_buffer = (expected_min_fee * (est.recommended_buffer_bps as i128)) / 10_000;
        assert_eq!(est.total_recommended_fee_stroops, expected_min_fee + expected_buffer);
        assert!(est.total_recommended_fee_stroops > est.min_fee_stroops);
    }
}

#[test]
fn test_estimate_payroll_resources_zero_recipients() {
    let env = Env::default();
    let est = estimate_payroll_resources_calc(&env, 0);

    assert_eq!(est.recipient_count, 0);
    assert_eq!(est.cpu_instructions, 850_000);
    assert_eq!(est.mem_bytes, 250_000);
    assert_eq!(est.min_fee_stroops, 50_000);
    assert_eq!(est.recommended_buffer_bps, 1500);
}

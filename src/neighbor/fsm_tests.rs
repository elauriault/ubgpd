use super::fsm::calculate_retry_delay;

#[test]
fn test_calculate_retry_delay_fixed() {
    let delay1 = calculate_retry_delay(10, 0, false);
    let delay2 = calculate_retry_delay(10, 3, false);

    assert!((10..=11).contains(&delay1));
    assert!((10..=11).contains(&delay2));
}

#[test]
fn test_calculate_retry_delay_exponential() {
    let delay1 = calculate_retry_delay(5, 0, true);
    let delay2 = calculate_retry_delay(5, 1, true);
    let delay3 = calculate_retry_delay(5, 2, true);

    assert!(delay1 == 5);
    assert!((10..=11).contains(&delay2));
    assert!((20..=22).contains(&delay3));
}

#[test]
fn test_exponential_backoff_caps_at_max() {
    let delay = calculate_retry_delay(60, 20, true);
    assert!(delay <= 3600);
}

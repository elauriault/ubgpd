use super::fsm::calculate_retry_delay;

#[test]
fn test_calculate_retry_delay_fixed() {
    let delay1 = calculate_retry_delay(10, 0, false);
    let delay2 = calculate_retry_delay(10, 3, false);
    
    assert!(delay1 >= 10 && delay1 <= 11);
    assert!(delay2 >= 10 && delay2 <= 11);
}

#[test]
fn test_calculate_retry_delay_exponential() {
    let delay1 = calculate_retry_delay(5, 0, true);
    let delay2 = calculate_retry_delay(5, 1, true);
    let delay3 = calculate_retry_delay(5, 2, true);
    
    assert!(delay1 >= 5 && delay1 <= 5);
    assert!(delay2 >= 10 && delay2 <= 11);
    assert!(delay3 >= 20 && delay3 <= 22);
}

#[test]
fn test_exponential_backoff_caps_at_max() {
    let delay = calculate_retry_delay(60, 20, true);
    assert!(delay <= 3600);
}
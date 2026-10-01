use super::parse_query;

#[test]
fn parse_query_accepts_quoted_names_and_unavailable_metrics() {
    let query = parse_query(b"1,GPU-b,\"NVIDIA, Test\",8192,N/A,[N/A]\n0,GPU-a,NVIDIA A,4096,75,1024\n").expect("valid query");
    assert_eq!(query.devices[0].uuid, "GPU-a");
    assert_eq!(query.devices[1].name, "NVIDIA, Test");
    assert_eq!(query.readings[0].utilization_percent, Some(75));
    assert_eq!(query.readings[1].memory_used_mib, None);
}
#[test]
fn parse_query_rejects_empty_malformed_and_out_of_range_results() {
    assert!(parse_query(b"").is_err());
    assert!(parse_query(b"0,GPU-a,name\n").is_err());
    assert!(parse_query(b"0,GPU-a,name,4096,101,1\n").is_err());
}

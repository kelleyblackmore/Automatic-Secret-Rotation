use secret_rotator::targets::PostgresTarget;

#[test]
fn test_quote_identifier() {
    assert_eq!(
        PostgresTarget::quote_identifier("test_user"),
        "\"test_user\""
    );
    assert_eq!(
        PostgresTarget::quote_identifier("user\"name"),
        "\"user\"\"name\""
    );
}

#[test]
fn test_build_connection_string() {
    let conn_str = PostgresTarget::build_connection_string(
        "localhost",
        5432,
        "postgres",
        "password",
        "postgres",
        "prefer",
    );
    assert!(conn_str.contains("host=localhost"));
    assert!(conn_str.contains("port=5432"));
    assert!(conn_str.contains("user=postgres"));
    assert!(conn_str.contains("password=password"));
    assert!(conn_str.contains("dbname=postgres"));
    assert!(conn_str.contains("sslmode=prefer"));
}
